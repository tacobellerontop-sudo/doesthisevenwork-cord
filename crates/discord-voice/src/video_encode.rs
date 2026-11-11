//! Platform hardware H.264 encoders shared by screen sharing and the camera.
//!
//! Media Foundation on Windows and VideoToolbox on macOS; Linux encodes inside its GStreamer
//! pipelines instead. Every backend is optional: callers keep an openh264 encoder for the
//! machines and formats the hardware refuses, and fall back mid-stream when one fails.

#[path = "video_encode_windows.rs"]
pub(crate) mod windows;

pub(crate) use windows as hardware;

/// One hardware encoder's output format. Bounds come from the caller because a screen frame
/// and a camera frame have very different limits.
#[derive(Clone, Copy)]
pub(crate) struct Config {
	pub width: u32,
	pub height: u32,
	pub fps: u32,
	pub bit_rate: u32,
	/// Largest encoded access unit the caller's transport accepts.
	pub max_bytes: usize,
	/// Screen sharing already ships Main; the camera keeps the Baseline profile its
	/// software encoder has always sent, so its wire format is unchanged.
	pub profile: Profile,
}

/// H.264 profile requested from the hardware encoder.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
	Baseline,
	Main,
}
