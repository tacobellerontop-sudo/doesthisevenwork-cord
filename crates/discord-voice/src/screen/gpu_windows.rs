//! GPU scaling and colour conversion for captured screen frames.
//!
//! Windows Graphics Capture hands over a BGRA texture that already lives on the GPU. The
//! Direct3D 11 video processor scales and letterboxes it to the stream size and converts it to
//! limited-range BT.601 NV12 in one pass, so only the encoder-ready picture (1.5 bytes per
//! stream pixel instead of 4 bytes per source pixel) is read back, and no per-pixel work runs
//! on the CPU.
//!
//! Any failure leaves the caller on the CPU path ([`super::convert`]). The first converted
//! frame is also checked against the CPU conversion of the same picture, so a driver that
//! produces the wrong colours or geometry is never used for a share.
#![allow(unsafe_code)]

use crate::screen::{FrameFormat, RawFrame};
use windows::{
	Win32::{
		Foundation::RECT,
		Graphics::{
			Direct3D11::*,
			Dxgi::Common::{
				DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
				DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601, DXGI_FORMAT_B8G8R8A8_UNORM,
				DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
			},
		},
	},
	core::Interface,
};

/// Largest mean difference per sample (on 0-255) accepted between the GPU and CPU conversions
/// of the same frame. Scalers and chroma siting differ slightly; wrong colour or geometry
/// differs by far more.
const MAX_MEAN_DIFFERENCE: u64 = 6;

/// Video processor state for one source size; rebuilt when the source is resized.
pub(super) struct Converter {
	_device: ID3D11Device,
	context: ID3D11DeviceContext,
	video: ID3D11VideoContext,
	video_device: ID3D11VideoDevice,
	enumerator: ID3D11VideoProcessorEnumerator,
	processor: ID3D11VideoProcessor,
	output_view: ID3D11VideoProcessorOutputView,
	output: ID3D11Texture2D,
	staging: ID3D11Texture2D,
	source: (u32, u32),
	target: (u32, u32),
	/// Set once the first frame matched the CPU conversion.
	pub(super) verified: bool,
}

/// Borrow a COM interface from `windows-capture`'s copy of the `windows` crate as ours.
fn adopt<T: Interface>(raw: *mut core::ffi::c_void) -> Option<T> {
	// SAFETY: `raw` is a live interface pointer of type `T` owned by the capture frame for this
	// call; `from_raw_borrowed` adds no reference and `cloned` takes our own.
	unsafe { T::from_raw_borrowed(&raw).cloned() }
}

impl Converter {
	/// Builds a converter on the capture's own device for `source` -> `target` (stream size).
	pub(super) fn new(
		device_raw: *mut core::ffi::c_void,
		context_raw: *mut core::ffi::c_void,
		source: (u32, u32),
		target: (u32, u32),
		fps: u32,
	) -> Option<Self> {
		if target.0 == 0
			|| target.1 == 0
			|| !target.0.is_multiple_of(2)
			|| !target.1.is_multiple_of(2)
		{
			return None;
		}
		let device: ID3D11Device = adopt(device_raw)?;
		let context: ID3D11DeviceContext = adopt(context_raw)?;
		let video_device: ID3D11VideoDevice = device.cast().ok()?;
		let video: ID3D11VideoContext = context.cast().ok()?;
		// SAFETY: Plain D3D11 object creation on the capture device with initialized
		// descriptors; every out-pointer refers to an initialized local.
		unsafe {
			let rate = DXGI_RATIONAL {
				Numerator: fps.max(1),
				Denominator: 1,
			};
			let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
				InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
				InputFrameRate: rate,
				InputWidth: source.0,
				InputHeight: source.1,
				OutputFrameRate: rate,
				OutputWidth: target.0,
				OutputHeight: target.1,
				Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
			};
			let enumerator = video_device.CreateVideoProcessorEnumerator(&content).ok()?;
			let input_support = enumerator
				.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)
				.ok()?;
			let output_support = enumerator
				.CheckVideoProcessorFormat(DXGI_FORMAT_NV12)
				.ok()?;
			if input_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 == 0
				|| output_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0
			{
				return None;
			}
			let processor = video_device.CreateVideoProcessor(&enumerator, 0).ok()?;
			let texture = |usage, bind: u32, cpu: u32| {
				let desc = D3D11_TEXTURE2D_DESC {
					Width: target.0,
					Height: target.1,
					MipLevels: 1,
					ArraySize: 1,
					Format: DXGI_FORMAT_NV12,
					SampleDesc: DXGI_SAMPLE_DESC {
						Count: 1,
						Quality: 0,
					},
					Usage: usage,
					BindFlags: bind,
					CPUAccessFlags: cpu,
					MiscFlags: 0,
				};
				let mut texture = None;
				device
					.CreateTexture2D(&desc, None, Some(&mut texture))
					.ok()
					.and(texture)
			};
			let output = texture(D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0)?;
			let staging = texture(D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32)?;
			let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
				ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
				Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
					Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
				},
			};
			let mut output_view = None;
			video_device
				.CreateVideoProcessorOutputView(
					&output,
					&enumerator,
					&view_desc,
					Some(&mut output_view),
				)
				.ok()?;
			let output_view = output_view?;

			// Full-range sRGB in, limited-range BT.601 out: what the CPU path and every
			// receiver (WebRTC's untagged default) assume.
			match video.cast::<ID3D11VideoContext1>() {
				Ok(video1) => {
					video1.VideoProcessorSetStreamColorSpace1(
						&processor,
						0,
						DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
					);
					video1.VideoProcessorSetOutputColorSpace1(
						&processor,
						DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601,
					);
				}
				Err(_) => {
					// Bitfield: Usage:1, RGB_Range:1, YCbCr_Matrix:1, YCbCr_xvYCC:1, Nominal_Range:2.
					let input = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 };
					let output = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
						_bitfield: (D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_16_235.0 as u32) << 4,
					};
					video.VideoProcessorSetStreamColorSpace(&processor, 0, &input);
					video.VideoProcessorSetOutputColorSpace(&processor, &output);
				}
			}
			video.VideoProcessorSetStreamFrameFormat(
				&processor,
				0,
				D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
			);
			// No driver "enhancements" (sharpening, noise reduction) on desktop content.
			video.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
			// Letterbox exactly like the CPU path: fit, centre, black bars.
			let (scaled_width, scaled_height) = crate::screen::convert::fitted(source, target);
			let left = (target.0 - scaled_width) / 2;
			let top = (target.1 - scaled_height) / 2;
			let whole = |width: u32, height: u32| RECT {
				left: 0,
				top: 0,
				right: width as i32,
				bottom: height as i32,
			};
			video.VideoProcessorSetStreamSourceRect(
				&processor,
				0,
				true,
				Some(&whole(source.0, source.1)),
			);
			video.VideoProcessorSetStreamDestRect(
				&processor,
				0,
				true,
				Some(&RECT {
					left: left as i32,
					top: top as i32,
					right: (left + scaled_width) as i32,
					bottom: (top + scaled_height) as i32,
				}),
			);
			video.VideoProcessorSetOutputTargetRect(
				&processor,
				true,
				Some(&whole(target.0, target.1)),
			);
			let black = D3D11_VIDEO_COLOR {
				Anonymous: D3D11_VIDEO_COLOR_0 {
					YCbCr: D3D11_VIDEO_COLOR_YCbCrA {
						Y: 16.0 / 255.0,
						Cb: 0.5,
						Cr: 0.5,
						A: 1.0,
					},
				},
			};
			video.VideoProcessorSetOutputBackgroundColor(&processor, true, &black);
			Some(Self {
				_device: device,
				context,
				video,
				video_device,
				enumerator,
				processor,
				output_view,
				output,
				staging,
				source,
				target,
				verified: false,
			})
		}
	}

	pub(super) fn matches(&self, source: (u32, u32)) -> bool {
		self.source == source
	}

	/// Converts the captured texture and reads the NV12 picture back into `data` (cleared and
	/// refilled, so a recycled buffer keeps its allocation).
	pub(super) fn convert(
		&self,
		texture_raw: *mut core::ffi::c_void,
		mut data: Vec<u8>,
	) -> Option<RawFrame> {
		let texture: ID3D11Texture2D = adopt(texture_raw)?;
		let (width, height) = (self.target.0 as usize, self.target.1 as usize);
		// SAFETY: D3D11 calls on the capture device inside its frame callback, the only
		// thread that uses this context. The mapped pointer is read only between Map and
		// Unmap, within the bounds computed from the reported row pitch.
		unsafe {
			let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
				FourCC: 0,
				ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
				Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
					Texture2D: D3D11_TEX2D_VPIV {
						MipSlice: 0,
						ArraySlice: 0,
					},
				},
			};
			let mut input_view = None;
			self.video_device
				.CreateVideoProcessorInputView(
					&texture,
					&self.enumerator,
					&input_desc,
					Some(&mut input_view),
				)
				.ok()?;
			let stream = D3D11_VIDEO_PROCESSOR_STREAM {
				Enable: true.into(),
				pInputSurface: std::mem::ManuallyDrop::new(input_view),
				..Default::default()
			};
			let streams = [stream];
			let blit =
				self.video
					.VideoProcessorBlt(&self.processor, &self.output_view, 0, &streams);
			let [stream] = streams;
			drop(std::mem::ManuallyDrop::into_inner(stream.pInputSurface));
			blit.ok()?;
			self.context.CopyResource(&self.staging, &self.output);
			let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
			self.context
				.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
				.ok()?;
			let pitch = mapped.RowPitch as usize;
			let result = (|| {
				if mapped.pData.is_null() || pitch < width {
					return None;
				}
				// NV12 staging layout: `height` luma rows, then `height / 2` chroma rows,
				// both at the same row pitch.
				let rows = height + height / 2;
				let source = std::slice::from_raw_parts(
					mapped.pData.cast::<u8>(),
					pitch * (rows - 1) + width,
				);
				data.clear();
				data.reserve_exact(width * rows);
				if pitch == width {
					data.extend_from_slice(&source[..width * rows]);
				} else {
					for row in 0..rows {
						data.extend_from_slice(&source[row * pitch..][..width]);
					}
				}
				Some(())
			})();
			self.context.Unmap(&self.staging, 0);
			result?;
		}
		Some(RawFrame {
			width: self.target.0,
			height: self.target.1,
			stride: width,
			format: FrameFormat::Nv12,
			data,
		})
	}

	/// Whether `gpu` (this converter's output) matches the CPU conversion of the same `bgra`
	/// source closely enough to stream it.
	pub(super) fn agrees(&self, bgra: &RawFrame, gpu: &RawFrame) -> bool {
		let (width, height) = (self.target.0 as usize, self.target.1 as usize);
		let mut cpu = vec![0; width * height * 3 / 2];
		if crate::screen::convert::bgra_to_yuv420(
			bgra,
			width,
			height,
			crate::screen::convert::Chroma::Interleaved,
			&mut cpu,
		)
		.is_err() || cpu.len() != gpu.data.len()
		{
			return false;
		}
		mean_difference(&cpu, &gpu.data) <= MAX_MEAN_DIFFERENCE
	}
}

/// Mean absolute difference per byte, rounded down.
fn mean_difference(a: &[u8], b: &[u8]) -> u64 {
	if a.is_empty() || a.len() != b.len() {
		return u64::MAX;
	}
	let total: u64 = a
		.iter()
		.zip(b)
		.map(|(x, y)| u64::from(x.abs_diff(*y)))
		.sum();
	total / a.len() as u64
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn gpu_check_rejects_unrelated_pictures() {
		assert_eq!(mean_difference(&[16, 128], &[18, 126]), 2);
		assert_eq!(mean_difference(&[16, 128], &[235, 128]), 109);
		assert_eq!(mean_difference(&[1], &[]), u64::MAX);
	}
}
