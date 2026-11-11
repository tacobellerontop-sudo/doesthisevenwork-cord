pub(super) const MAX_SOURCES: usize = 64;
pub(super) const MAX_NAME_BYTES: usize = 256;
pub(super) const MAX_SOURCE_WIDTH: u32 = 7680;
pub(super) const MAX_SOURCE_HEIGHT: u32 = 4320;
pub(super) const MAX_FRAME_WIDTH: u32 = 3840;
pub(super) const MAX_FRAME_HEIGHT: u32 = 2160;
pub(super) const MAX_RAW_BYTES: usize = MAX_FRAME_WIDTH as usize * MAX_FRAME_HEIGHT as usize * 4;

pub(super) fn bounded_name(mut name: String) -> String {
	if name.len() > MAX_NAME_BYTES {
		let mut end = MAX_NAME_BYTES;
		while !name.is_char_boundary(end) {
			end -= 1;
		}
		name.truncate(end);
		name.shrink_to_fit();
	}
	name
}

#[path = "capture_windows.rs"]
mod imp;

pub(crate) use imp::{Capture, sources};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn source_names_end_on_utf8_boundaries() {
		let name = bounded_name(format!("{}é", "a".repeat(MAX_NAME_BYTES - 1)));
		assert_eq!(name.len(), MAX_NAME_BYTES - 1);
	}
}
