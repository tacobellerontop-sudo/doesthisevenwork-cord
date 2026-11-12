//! Explicitly selected, memory-only screen capture and H.264 encoding.
pub use client_core::screen::{Settings, Source, SourceId};
#[path = "screen/capture.rs"]
mod capture;
#[path = "screen/convert.rs"]
mod convert;

use openh264::{
	OpenH264API,
	encoder::{
		BitRate, Complexity, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod,
		RateControlMode, UsageType,
	},
	formats::{BgraSliceU8, YUVBuffer, YUVSource},
};
use std::borrow::Cow;
use std::sync::{
	Arc, Mutex,
	atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
	mpsc,
};

use std::time::{Duration, Instant};

pub const MAX_RAW_BYTES: usize = 3840 * 2160 * 4;
pub const MAX_ENCODED_BYTES: usize = 2 * 1024 * 1024;

/// One captured frame's pixels handed back from the encoder for the capture to refill.
pub(crate) type Spare = Arc<Mutex<Option<Vec<u8>>>>;

/// Keep `data` for the next capture unless one buffer is already waiting.
pub(crate) fn give_back(spare: &Spare, data: Vec<u8>) {
	if let Ok(mut spare) = spare.lock()
		&& spare.is_none()
	{
		*spare = Some(data);
	}
}

pub struct RawFrame {
	pub width: u32,
	pub height: u32,
	/// Bytes per row of the first (or only) plane.
	pub stride: usize,
	pub format: FrameFormat,
	pub data: Vec<u8>,
}

/// Pixel layout of a [`RawFrame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameFormat {
	/// Packed BGRA at the captured source size, converted on the CPU before encoding.
	Bgra,
	/// Limited-range BT.601 NV12 already scaled and letterboxed to the stream size by the
	/// GPU: tightly packed luma (`stride == width`), then interleaved Cb/Cr.
	Nv12,
}

pub struct EncodedFrame {
	pub data: Vec<u8>,
	pub timestamp: u32,
	pub keyframe: bool,
}

/// Longest system-audio chunk accepted from the OS: 100 ms of 48 kHz stereo.
pub const MAX_AUDIO_SAMPLES: usize = 4800 * 2;

#[derive(Debug)]
pub struct AudioChunk {
	pub samples: Vec<f32>,
	/// Capture generation; old buffers must not cross an encryption transition.
	pub epoch: u64,
}

pub struct Video {
	pub settings: Settings,
	pub frames: tokio::sync::mpsc::Receiver<EncodedFrame>,
	pub ready: Arc<AtomicBool>,
	pub keyframe: Arc<AtomicBool>,
	/// Transport feedback target in bits per second, bounded by the selected quality.
	pub bitrate: Arc<AtomicU32>,
	/// Interleaved 48 kHz stereo system audio, present only when the share requested it.
	pub audio: Option<tokio::sync::mpsc::Receiver<AudioChunk>>,
	pub audio_epoch: Arc<AtomicU64>,
}

/// Whether this platform can capture system audio with the screen.
pub fn audio_supported() -> bool {
	supported()
}

pub fn supported() -> bool {
	true
}

pub fn sources() -> Result<Vec<Source>, &'static str> {
	capture::sources()
}

pub struct Worker {
	stop: Arc<AtomicBool>,
	ready: Arc<AtomicBool>,
	preview: Arc<Mutex<Option<image::RgbaImage>>>,
	done: Option<mpsc::Receiver<Result<(), Cow<'static, str>>>>,
	preview_visible: Arc<AtomicBool>,
	captured: Arc<AtomicBool>,
}

impl Worker {
	pub fn start(
		settings: Settings,
		wake: impl Fn() + Send + 'static,
	) -> Result<(Self, Video), &'static str> {
		if !settings.valid() || !supported() {
			return Err("Screen sharing is unavailable for these settings or this platform");
		}
		let stop = Arc::new(AtomicBool::new(false));
		let ready = Arc::new(AtomicBool::new(false));
		let audio_epoch = Arc::new(AtomicU64::new(0));
		let worker_audio_epoch = audio_epoch.clone();
		let keyframe = Arc::new(AtomicBool::new(true));
		let bitrate = Arc::new(AtomicU32::new(settings.bit_rate()));
		let worker_bitrate = bitrate.clone();
		let preview = Arc::new(Mutex::new(None));
		let worker_preview = preview.clone();
		let preview_visible = Arc::new(AtomicBool::new(true));
		let worker_preview_visible = preview_visible.clone();
		let captured = Arc::new(AtomicBool::new(false));
		let worker_captured = captured.clone();
		// A few frames of slack absorbs send jitter without forcing keyframes on every hiccup.
		let (send, frames) = tokio::sync::mpsc::channel(3);
		let (complete, done) = mpsc::sync_channel(1);
		let (audio_send, audio) = if settings.audio && audio_supported() {
			let (send, receive) = tokio::sync::mpsc::channel(4);
			(Some(send), Some(receive))
		} else {
			(None, None)
		};
		let (worker_stop, worker_ready, worker_keyframe) =
			(stop.clone(), ready.clone(), keyframe.clone());
		let teardown = crate::teardown::Thread::start();
		std::thread::Builder::new()
			.name("screen-encoder".into())
			.spawn(move || {
				// Declared first so it is released last, after the pipeline and encoder.
				let _teardown = teardown;
				// The MFT event poll and capture pacing sleep 1 ms at a time.
				let _resolution = crate::timer::Resolution::acquire();
				let finished_ready = worker_ready.clone();
				let result = encode_loop(
					settings,
					worker_stop,
					worker_ready,
					worker_keyframe,
					worker_bitrate,
					send,
					audio_send,
					worker_audio_epoch,
					worker_preview,
					worker_preview_visible,
					worker_captured,
					&wake,
				);
				finished_ready.store(false, Ordering::Release);
				// The desktop only shows the latest status, which a later stop overwrites.
				// Name the cause once so a share that ends by itself is never a mystery.
				if std::env::var_os("DTEWCORD_VOICE_DIAGNOSTICS").is_some_and(|value| value == "1")
				{
					match &result {
						Ok(()) => eprintln!("[DTEW Cord voice Screen] capture_stopped=ok"),
						Err(reason) => {
							eprintln!("[DTEW Cord voice Screen] capture_stopped={reason}")
						}
					}
				}
				let _ = complete.try_send(result);
				wake();
			})
			.map_err(|_| "Could not start screen capture worker")?;
		Ok((
			Self {
				stop,
				ready: ready.clone(),
				preview,
				done: Some(done),
				preview_visible,
				captured,
			},
			Video {
				settings,
				frames,
				ready,
				keyframe,
				bitrate,
				audio,
				audio_epoch,
			},
		))
	}

	pub fn set_preview_visible(&self, visible: bool) {
		self.preview_visible.store(visible, Ordering::Release);
	}

	/// True after capture has produced one frame. Linux can be waiting on the system picker before that.
	pub fn captured(&self) -> bool {
		self.captured.load(Ordering::Acquire)
	}

	pub fn capture_status(&self) -> Option<&'static str> {
		None
	}

	/// How the share ended, once it has. The reason is owned because it can carry a Windows
	/// error code that is only known at run time.
	pub fn result(&self) -> Option<Result<(), Cow<'static, str>>> {
		self.done.as_ref()?.try_recv().ok()
	}

	/// One local RGBA image at the captured size, replaced at the chosen frame rate while visible.
	pub fn take_preview(&self) -> Option<image::RgbaImage> {
		self.preview.try_lock().ok()?.take()
	}

	/// Stops the worker and hands over the channel that reports how it ended.
	pub fn shutdown(mut self) -> mpsc::Receiver<Result<(), Cow<'static, str>>> {
		self.done.take().expect("screen worker completion")
	}
}

impl Drop for Worker {
	fn drop(&mut self) {
		self.ready.store(false, Ordering::Release);
		self.stop.store(true, Ordering::Release);
	}
}

/// Captures, encodes and sends frames until the share stops or capture fails. A failure's
/// reason is owned so it can carry the Windows error code of a failed audio capture.
#[allow(clippy::too_many_arguments)] // Media outputs of one explicitly started capture.
fn encode_loop(
	settings: Settings,
	stop: Arc<AtomicBool>,
	ready: Arc<AtomicBool>,
	keyframe: Arc<AtomicBool>,
	bitrate: Arc<AtomicU32>,
	send: tokio::sync::mpsc::Sender<EncodedFrame>,
	audio: Option<tokio::sync::mpsc::Sender<AudioChunk>>,
	audio_epoch: Arc<AtomicU64>,
	preview: Arc<Mutex<Option<image::RgbaImage>>>,
	preview_visible: Arc<AtomicBool>,
	captured: Arc<AtomicBool>,
	wake: &impl Fn(),
) -> Result<(), Cow<'static, str>> {
	if stop.load(Ordering::Acquire) || send.is_closed() {
		return Ok(());
	}
	let origin = Instant::now();
	let (raw_send, raw) = mpsc::sync_channel(1);
	let capture_stop = Arc::new(AtomicBool::new(false));
	let raw_pending = Arc::new(AtomicBool::new(false));
	let spare: Spare = Arc::default();
	let _native = capture::Capture::start(
		settings,
		raw_send,
		spare.clone(),
		audio,
		capture_stop.clone(),
		ready.clone(),
		audio_epoch,
		raw_pending.clone(),
		stop.clone(),
	)?;
	// The worker's stop also reaches a pending macOS picker, so call teardown closes it now.
	let mut encoding = None;
	let mut first_frame_deadline = Some(Instant::now() + Duration::from_secs(15));
	let mut next_frame = Instant::now();
	let mut latest_frame = None;
	let mut preview_shown = preview_visible.load(Ordering::Acquire);
	let mut next_preview = Instant::now();
	// A captured change was skipped for pacing and the preview has not shown it yet.
	let mut preview_behind = false;

	while !stop.load(Ordering::Acquire) && !send.is_closed() {
		if _native.failed() {
			return Err(_native.failure());
		}
		if capture_stop.load(Ordering::Acquire) {
			return Err("The selected screen or window stopped sharing".into());
		}
		let frame = match raw.recv_timeout(Duration::from_millis(100)) {
			Ok(frame) => {
				raw_pending.store(false, Ordering::Release);
				Some(frame)
			}
			Err(_) if stop.load(Ordering::Acquire) || send.is_closed() => break,
			Err(mpsc::RecvTimeoutError::Timeout)
				if first_frame_deadline.is_none_or(|deadline| Instant::now() < deadline) =>
			{
				None
			}
			Err(_) => {
				return Err(
					"No screen frames received; check screen recording permission and the selected source"
						.into(),
				);
			}
		};
		if stop.load(Ordering::Acquire) || send.is_closed() {
			break;
		}
		let now = Instant::now();
		let visible = preview_visible.load(Ordering::Acquire);
		let resumed = visible && !preview_shown;
		preview_shown = visible;
		if let Some(frame) = &frame {
			if first_frame_deadline.take().is_some() {
				captured.store(true, Ordering::Release);
			}
			if !visible {
				if let Ok(mut slot) = preview.try_lock() {
					slot.take();
				}
			} else if now >= next_preview {
				next_preview = now + PREVIEW_INTERVAL;
				preview_behind = false;
				let image = preview_frame(frame)?;
				if let Ok(mut slot) = preview.try_lock() {
					*slot = Some(image);
				}
				wake();
			} else {
				preview_behind = true;
			}
		} else if visible
			&& (resumed || preview_behind)
			&& let Some(frame) = &latest_frame
		{
			// A change-driven capture does not emit again while the picture is still, so
			// show the retained frame on return or when the last change arrived too soon.
			// The encode path already kept that frame.
			next_preview = now + PREVIEW_INTERVAL;
			preview_behind = false;
			let image = preview_frame(frame)?;
			if let Ok(mut slot) = preview.try_lock() {
				*slot = Some(image);
			}
			wake();
		}
		if frame.is_some()
			&& let Some(old) = latest_frame.take()
		{
			give_back(&spare, old.data);
		}
		let encode = retain_screen_frame(
			&mut latest_frame,
			frame,
			ready.load(Ordering::Acquire),
			keyframe.load(Ordering::Acquire),
		)?;
		// Local capture remains available while alone; only secure media is encoded or queued.
		if !ready.load(Ordering::Acquire) {
			encoding = None;
			keyframe.store(true, Ordering::Release);
			continue;
		}
		if !encode {
			continue;
		}
		// Pace on an accumulating schedule with a little tolerance: capture timing jitter must
		// not skip every other frame, and a stalled encoder resumes from now instead of bursting.
		let interval = Duration::from_secs_f64(1.0 / f64::from(settings.fps));
		if now + Duration::from_millis(2) < next_frame {
			continue;
		}
		next_frame = (next_frame + interval).max(now + interval / 2);
		// Drop before encoding while the transport is behind, so the encoder never references
		// a picture the receiver did not get.
		if send.capacity() == 0 {
			continue;
		}
		let target = bitrate
			.load(Ordering::Acquire)
			.clamp(250_000, settings.bit_rate());
		if encoding.is_none() {
			encoding = Some(ScreenEncoder::new(settings, target)?);
		}
		if encoding
			.as_mut()
			.expect("secure screen encoder")
			.set_bitrate(target)?
		{
			keyframe.store(true, Ordering::Release);
		}
		// Retain one current source snapshot, never encoded media, for a viewer's keyframe
		// request on an unchanged desktop. VideoToolbox takes packed BGRA at the stream size
		// (ScreenCaptureKit already scales); other encoders convert the source in one pass.
		let force_keyframe = keyframe.swap(false, Ordering::AcqRel);
		let (data, is_keyframe) = encoding.as_mut().expect("secure screen encoder").encode(
			latest_frame.as_ref().expect("latest screen frame"),
			force_keyframe,
		)?;
		if force_keyframe && (data.is_empty() || !is_keyframe) {
			keyframe.store(true, Ordering::Release);
		}
		if data.is_empty() {
			continue;
		}
		if !ready.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
			continue;
		}
		let frame = EncodedFrame {
			data,
			timestamp: (origin.elapsed().as_micros() * 90 / 1000) as u32,
			keyframe: is_keyframe,
		};
		if send.try_send(frame).is_err() {
			keyframe.store(true, Ordering::Release);
		}
	}
	capture_stop.store(true, Ordering::Release);
	Ok(())
}

fn retain_screen_frame(
	latest: &mut Option<RawFrame>,
	frame: Option<RawFrame>,
	ready: bool,
	keyframe: bool,
) -> Result<bool, &'static str> {
	let fresh = frame.is_some();
	if let Some(frame) = frame {
		validate_frame(&frame)?;
		*latest = Some(frame);
	}
	Ok(ready && latest.is_some() && (fresh || keyframe))
}

/// Screen encoder preferring the platform hardware H.264 encoder (Media Foundation on
/// Windows, VideoToolbox on macOS) and falling back to openh264 when it is unavailable or
/// fails mid-stream.
struct ScreenEncoder {
	diagnostics: crate::diagnostics::EncoderRegistration,
	software: Option<Encoder>,
	/// Reused I420 picture for openh264.
	i420: Vec<u8>,
	hardware: Option<crate::video_encode::hardware::Encoder>,
	settings: Settings,
	bitrate: u32,
}

impl ScreenEncoder {
	fn new(settings: Settings, bitrate: u32) -> Result<Self, &'static str> {
		let config = crate::video_encode::Config {
			width: settings.width,
			height: settings.height,
			fps: settings.fps,
			bit_rate: bitrate,
			max_bytes: MAX_ENCODED_BYTES,
			profile: crate::video_encode::Profile::Main,
		};
		let hardware = crate::video_encode::hardware::Encoder::new(config).ok();
		let software = if hardware.is_none() {
			Some(encoder(settings, bitrate)?)
		} else {
			None
		};
		Ok(Self {
			diagnostics: crate::diagnostics::EncoderRegistration::new(true, hardware.is_some()),
			software,
			i420: Vec::new(),
			hardware,
			settings,
			bitrate,
		})
	}

	fn set_bitrate(&mut self, bitrate: u32) -> Result<bool, &'static str> {
		if self.bitrate == bitrate {
			return Ok(false);
		}
		if self
			.hardware
			.as_mut()
			.is_some_and(|encoder| encoder.set_bitrate(bitrate).is_ok())
		{
			self.bitrate = bitrate;
			return Ok(false);
		}
		if self.hardware.is_none() {
			// OpenH264 exposes no safe runtime bitrate setter, and a restart costs an IDR:
			// follow only large moves so gradual recovery cannot cause a keyframe per second.
			if !software_rate_change(self.bitrate, bitrate) {
				return Ok(false);
			}
			self.software = Some(encoder(self.settings, bitrate)?);
			self.bitrate = bitrate;
		} else {
			// Reopen native encoders that refuse a live rate change at the target rate.
			// Release scarce hardware sessions before requesting their replacement.
			self.hardware = None;
			*self = Self::new(self.settings, bitrate)?;
		}
		Ok(true)
	}

	fn encode(
		&mut self,
		frame: &RawFrame,
		force_keyframe: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		let (width, height) = (self.settings.width as usize, self.settings.height as usize);
		let mut software_force = force_keyframe;
		if let Some(hardware) = self.hardware.as_mut() {
			let encoded = hardware.encode_with(width * height * 3 / 2, force_keyframe, |picture| {
				convert::bgra_to_yuv420(frame, width, height, convert::Chroma::Interleaved, picture)
			});
			if let Ok(encoded) = encoded {
				return Ok(encoded);
			}
			// The viewer must restart from a keyframe once the software encoder takes over.
			self.hardware = None;
			self.diagnostics.set(None);
			self.software = Some(encoder(self.settings, self.bitrate)?);
			self.diagnostics.set(Some(false));
			software_force = true;
		}
		self.i420.resize(width * height * 3 / 2, 0);
		convert::bgra_to_yuv420(
			frame,
			width,
			height,
			convert::Chroma::Planar,
			&mut self.i420,
		)?;
		let (y, chroma) = self.i420.split_at(width * height);
		let (u, v) = chroma.split_at(width * height / 4);
		encode_yuv(
			self.software.as_mut().expect("software screen encoder"),
			&openh264::formats::YUVSlices::new(
				(y, u, v),
				(width, height),
				(width, width / 2, width / 2),
			),
			software_force,
		)
	}
}

/// Whether a software encoder should restart for a new transport target.
pub(crate) fn software_rate_change(current: u32, target: u32) -> bool {
	// Always honor congestion cuts of at least 15%; grow in 25% steps.
	u64::from(target) * 100 <= u64::from(current) * 85
		|| u64::from(target) * 100 >= u64::from(current) * 125
}

pub(crate) fn i420_to_nv12(
	y: &[u8],
	u: &[u8],
	v: &[u8],
	output: &mut [u8],
) -> Result<(), &'static str> {
	if u.len() != v.len() || y.len() != u.len() * 4 || output.len() != y.len() + u.len() + v.len() {
		return Err("Invalid screen encoder color planes");
	}
	output[..y.len()].copy_from_slice(y);
	for (pair, (&u, &v)) in output[y.len()..]
		.as_chunks_mut::<2>()
		.0
		.iter_mut()
		.zip(u.iter().zip(v))
	{
		pair.copy_from_slice(&[u, v]);
	}
	Ok(())
}

pub(super) fn encoder(settings: Settings, bitrate: u32) -> Result<Encoder, &'static str> {
	let config = EncoderConfig::new()
		.bitrate(BitRate::from_bps(
			bitrate.clamp(250_000, settings.bit_rate()),
		))
		.max_frame_rate(FrameRate::from_hz(settings.fps as f32))
		.usage_type(UsageType::ScreenContentRealTime)
		.rate_control_mode(RateControlMode::Bitrate)
		.complexity(Complexity::Low)
		.num_threads(encoder_threads())
		.intra_frame_period(IntraFramePeriod::from_num_frames(settings.fps * 2));
	Encoder::with_api_config(OpenH264API::from_source(), config)
		.map_err(|_| "Screen video encoder is unavailable")
}

fn encoder_threads() -> u16 {
	std::thread::available_parallelism().map_or(2, |count| count.get().clamp(2, 8) as u16)
}

#[allow(dead_code)]
pub(super) fn encode_pixels(
	encoder: &mut Encoder,
	yuv: &mut YUVBuffer,
	pixels: &[u8],
	dimensions: (usize, usize),
	force_keyframe: bool,
) -> Result<(Vec<u8>, bool), &'static str> {
	yuv.read_bgra8(BgraSliceU8::new(pixels, dimensions));
	encode_yuv(encoder, yuv, force_keyframe)
}

fn encode_yuv(
	encoder: &mut Encoder,
	yuv: &impl YUVSource,
	force_keyframe: bool,
) -> Result<(Vec<u8>, bool), &'static str> {
	if force_keyframe {
		encoder.force_intra_frame();
	}
	let encoded = encoder
		.encode(yuv)
		.map_err(|_| "Screen video encoding failed")?;
	let is_keyframe = matches!(encoded.frame_type(), FrameType::IDR);
	let mut encoded_len = 0usize;
	for layer_index in 0..encoded.num_layers() {
		let layer = encoded
			.layer(layer_index)
			.ok_or("Screen video encoder returned an invalid layer")?;
		for nal_index in 0..layer.nal_count() {
			encoded_len = encoded_len
				.checked_add(
					layer
						.nal_unit(nal_index)
						.ok_or("Screen video encoder returned an invalid NAL")?
						.len(),
				)
				.filter(|length| *length <= MAX_ENCODED_BYTES)
				.ok_or("Encoded screen frame exceeds the sharing limit; choose a lower quality")?;
		}
	}
	let mut data = Vec::with_capacity(encoded_len);
	encoded.write_vec(&mut data);
	Ok((data, is_keyframe))
}

fn validate_frame(frame: &RawFrame) -> Result<(usize, usize), &'static str> {
	if frame.format == FrameFormat::Nv12 {
		let (width, height) = (frame.width as usize, frame.height as usize);
		if width == 0
			|| height == 0
			|| width > 3840
			|| height > 2160
			|| !width.is_multiple_of(2)
			|| !height.is_multiple_of(2)
			|| frame.stride != width
			|| frame.data.len() != width * height * 3 / 2
		{
			return Err("Screen capture returned an unsupported frame size");
		}
		return Ok((width, frame.data.len()));
	}
	let row_bytes = (frame.width as usize)
		.checked_mul(4)
		.ok_or("Screen capture returned an unsupported frame size")?;
	let required = frame
		.stride
		.checked_mul(frame.height as usize)
		.ok_or("Screen capture returned an unsupported frame size")?;
	if frame.width == 0
		|| frame.height == 0
		|| frame.width > 3840
		|| frame.height > 2160
		|| frame.data.len() > MAX_RAW_BYTES
		|| frame.stride < row_bytes
		|| required > frame.data.len()
	{
		return Err("Screen capture returned an unsupported frame size");
	}

	Ok((row_bytes, required))
}

/// The local preview is a tile on screen, never the full desktop: bound it so a 4K share
/// does not convert, allocate and upload 33 MB per frame just to show itself.
const PREVIEW_MAX_WIDTH: u32 = 960;
const PREVIEW_MAX_HEIGHT: u32 = 540;
/// The local preview refreshes at most this often; each refresh costs a full UI frame.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);

pub(super) fn preview_frame(frame: &RawFrame) -> Result<image::RgbaImage, &'static str> {
	validate_frame(frame)?;
	if frame.format == FrameFormat::Nv12 {
		return preview_nv12(frame);
	}
	// Integer reduction: each preview pixel averages four samples spread over its block.
	let factor = frame
		.width
		.div_ceil(PREVIEW_MAX_WIDTH)
		.max(frame.height.div_ceil(PREVIEW_MAX_HEIGHT))
		.max(1) as usize;
	let (width, height) = (
		(frame.width as usize / factor).max(1),
		(frame.height as usize / factor).max(1),
	);
	let half = factor / 2;
	let (last_x, last_y) = (frame.width as usize - 1, frame.height as usize - 1);
	let mut pixels = Vec::with_capacity(width * height * 4);
	for y in 0..height {
		let top = (y * factor).min(last_y) * frame.stride;
		let bottom = (y * factor + half).min(last_y) * frame.stride;
		for x in 0..width {
			let (left, right) = (
				(x * factor).min(last_x) * 4,
				(x * factor + half).min(last_x) * 4,
			);
			let mut sum = [0u32; 3];
			for at in [top + left, top + right, bottom + left, bottom + right] {
				let source = &frame.data[at..at + 4];
				// BGRA in, RGB out.
				sum[0] += u32::from(source[2]);
				sum[1] += u32::from(source[1]);
				sum[2] += u32::from(source[0]);
			}
			pixels.extend(sum.map(|total| ((total + 2) / 4) as u8));
			pixels.push(255);
		}
	}
	image::RgbaImage::from_raw(width as u32, height as u32, pixels).ok_or("Invalid screen frame")
}

/// Preview of a GPU-converted NV12 frame: point samples (one per preview pixel) through
/// the same limited-range BT.601 matrix the stream uses.
fn preview_nv12(frame: &RawFrame) -> Result<image::RgbaImage, &'static str> {
	let (width, height) = (frame.width as usize, frame.height as usize);
	let factor = frame
		.width
		.div_ceil(PREVIEW_MAX_WIDTH)
		.max(frame.height.div_ceil(PREVIEW_MAX_HEIGHT))
		.max(1) as usize;
	let (out_width, out_height) = ((width / factor).max(1), (height / factor).max(1));
	let (luma, chroma) = frame.data.split_at(width * height);
	let mut pixels = Vec::with_capacity(out_width * out_height * 4);
	for y in 0..out_height {
		let source_y = (y * factor + factor / 2).min(height - 1);
		for x in 0..out_width {
			let source_x = (x * factor + factor / 2).min(width - 1);
			let c = i32::from(luma[source_y * width + source_x]) - 16;
			let at = (source_y / 2) * width + (source_x / 2) * 2;
			let (d, e) = (i32::from(chroma[at]) - 128, i32::from(chroma[at + 1]) - 128);
			let channel = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
			pixels.extend([
				channel(298 * c + 409 * e),
				channel(298 * c - 100 * d - 208 * e),
				channel(298 * c + 516 * d),
				255,
			]);
		}
	}
	image::RgbaImage::from_raw(out_width as u32, out_height as u32, pixels)
		.ok_or("Invalid screen frame")
}

#[cfg(test)]
fn fit_frame(frame: RawFrame, width: u32, height: u32) -> Result<Vec<u8>, &'static str> {
	let (row_bytes, required) = validate_frame(&frame)?;

	let mut packed = if frame.stride == row_bytes {
		frame.data
	} else {
		let mut packed = Vec::with_capacity(row_bytes * frame.height as usize);
		for row in frame.data[..required].chunks_exact(frame.stride) {
			packed.extend_from_slice(&row[..row_bytes]);
		}
		packed
	};
	packed.truncate(row_bytes * frame.height as usize);
	if frame.width == width && frame.height == height {
		return Ok(packed);
	}
	let image = image::RgbaImage::from_raw(frame.width, frame.height, packed)
		.ok_or("Invalid screen frame")?;
	let scale = (f64::from(width) / f64::from(frame.width))
		.min(f64::from(height) / f64::from(frame.height));
	let (scaled_width, scaled_height) = (
		(f64::from(frame.width) * scale).round().max(1.0) as u32,
		(f64::from(frame.height) * scale).round().max(1.0) as u32,
	);
	let scaled = image::imageops::resize(
		&image,
		scaled_width,
		scaled_height,
		image::imageops::FilterType::Triangle,
	);
	let mut output = image::RgbaImage::new(width, height);
	image::imageops::replace(
		&mut output,
		&scaled,
		i64::from((width - scaled_width) / 2),
		i64::from((height - scaled_height) / 2),
	);
	Ok(output.into_raw())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn idle_screen_keyframe_uses_latest_snapshot_only_when_ready() {
		let frame = |value| RawFrame {
			width: 2,
			height: 2,
			stride: 8,
			format: crate::screen::FrameFormat::Bgra,
			data: vec![value; 16],
		};
		let mut latest = None;
		assert!(!retain_screen_frame(&mut latest, None, true, true).unwrap());
		assert!(retain_screen_frame(&mut latest, Some(frame(1)), true, false).unwrap());
		assert!(!retain_screen_frame(&mut latest, None, true, false).unwrap());
		assert!(retain_screen_frame(&mut latest, None, true, true).unwrap());
		// A security pause keeps tracking the source without encoding. Its newest snapshot
		// can be freshly encoded after readiness, even if no further capture event arrives.
		assert!(!retain_screen_frame(&mut latest, Some(frame(2)), false, true).unwrap());
		assert!(!retain_screen_frame(&mut latest, None, false, true).unwrap());
		assert!(retain_screen_frame(&mut latest, None, true, true).unwrap());
		assert_eq!(latest.as_ref().unwrap().data, vec![2; 16]);
		let mut oversized = frame(3);
		oversized.width = 3841;
		assert!(retain_screen_frame(&mut latest, Some(oversized), true, true).is_err());
		assert_eq!(latest.unwrap().data, vec![2; 16]);
	}

	#[test]
	fn local_preview_is_bounded_and_converts_padded_bgra_without_media_readiness() {
		let preview = preview_frame(&RawFrame {
			width: 1,
			height: 2,
			stride: 8,
			format: crate::screen::FrameFormat::Bgra,
			data: vec![1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9],
		})
		.unwrap();
		assert_eq!(preview.as_raw(), &[3, 2, 1, 255, 6, 5, 4, 255]);
		let worker = Worker {
			stop: Arc::new(AtomicBool::new(false)),
			ready: Arc::new(AtomicBool::new(false)),
			preview: Arc::new(Mutex::new(Some(preview))),
			done: None,
			preview_visible: Arc::new(AtomicBool::new(true)),
			captured: Arc::new(AtomicBool::new(false)),
		};
		assert!(worker.take_preview().is_some());
		assert!(worker.take_preview().is_none());
		assert!(!worker.ready.load(Ordering::Acquire));
		for (width, height) in [(3840, 2160), (2, 2160), (3840, 2)] {
			let frame = RawFrame {
				width,
				height,
				stride: width as usize * 4,
				format: crate::screen::FrameFormat::Bgra,
				data: vec![0; width as usize * height as usize * 4],
			};
			let preview = preview_frame(&frame).unwrap();
			assert!(preview.width() <= PREVIEW_MAX_WIDTH && preview.height() <= PREVIEW_MAX_HEIGHT);
			assert!(preview.width() >= 1 && preview.height() >= 1);
			assert_eq!(
				preview.as_raw().len(),
				preview.width() as usize * preview.height() as usize * 4
			);
		}
		// A 4K desktop previews at a quarter size, averaging each block.
		let mut frame = RawFrame {
			width: 3840,
			height: 2160,
			stride: 3840 * 4,
			format: crate::screen::FrameFormat::Bgra,
			data: vec![0; 3840 * 2160 * 4],
		};
		// Top-left block: one white sample of the four taps (0,0),(2,0),(0,2),(2,2).
		frame.data[..4].copy_from_slice(&[255, 255, 255, 255]);
		let preview = preview_frame(&frame).unwrap();
		assert_eq!((preview.width(), preview.height()), (960, 540));
		assert_eq!(&preview.as_raw()[..4], &[64, 64, 64, 255]);
		// GPU NV12 frames preview through the same limited-range BT.601 matrix.
		let mut nv12 = vec![235; 4 * 4];
		nv12.extend([128; 8]);
		let preview = preview_frame(&RawFrame {
			width: 4,
			height: 4,
			stride: 4,
			format: FrameFormat::Nv12,
			data: nv12,
		})
		.unwrap();
		assert_eq!((preview.width(), preview.height()), (4, 4));
		assert_eq!(&preview.as_raw()[..4], &[255, 255, 255, 255]);
		// A short or mis-strided NV12 frame is refused.
		assert!(
			preview_frame(&RawFrame {
				width: 4,
				height: 4,
				stride: 8,
				format: FrameFormat::Nv12,
				data: vec![0; 24],
			})
			.is_err()
		);
		assert!(
			preview_frame(&RawFrame {
				width: 2,
				height: 2,
				stride: 4,
				format: crate::screen::FrameFormat::Bgra,
				data: vec![0; 8],
			})
			.is_err()
		);
	}

	#[test]
	fn synthetic_frame_is_bounded_and_encodes_a_keyframe() {
		let frame = RawFrame {
			width: 2,
			height: 2,
			stride: 12,
			format: crate::screen::FrameFormat::Bgra,
			data: vec![255; 24],
		};
		let pixels = fit_frame(frame, 1280, 720).unwrap();
		assert_eq!(pixels.len(), 1280 * 720 * 4);

		let settings = Settings {
			source: SourceId::Display(1),
			width: 1280,
			height: 720,
			fps: 30,
			cursor: true,
			audio: false,
		};
		let mut encoder = encoder(settings, settings.bit_rate()).unwrap();
		let mut yuv = YUVBuffer::new(1280, 720);
		let (encoded, keyframe) =
			encode_pixels(&mut encoder, &mut yuv, &pixels, (1280, 720), true).unwrap();
		assert!(keyframe);
		assert!(!encoded.is_empty() && encoded.len() <= MAX_ENCODED_BYTES);
		assert!(encoded.windows(5).any(|nal| nal == [0, 0, 0, 1, 0x65]));

		assert!(
			fit_frame(
				RawFrame {
					width: 2,
					height: 2,
					stride: 4,
					format: crate::screen::FrameFormat::Bgra,
					data: vec![0; 8],
				},
				1280,
				720,
			)
			.is_err()
		);
	}

	#[test]
	fn interleaves_i420_chroma_for_windows_nv12() {
		let mut nv12 = [0; 12];
		i420_to_nv12(&[1, 2, 3, 4, 5, 6, 7, 8], &[9, 10], &[11, 12], &mut nv12).unwrap();
		assert_eq!(nv12, [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 10, 12]);
		assert!(i420_to_nv12(&[0; 4], &[0; 2], &[0; 2], &mut [0; 8]).is_err());
	}
}
