use std::time::Duration;

/// Time since the last keyboard/mouse input anywhere on the system, `Duration::MAX` while the
/// session is locked, or `None` when this desktop exposes no idle source. Blocking; call off
/// the UI thread and async runtime.
pub fn system_idle() -> Option<Duration> {
	if session_locked() {
		return Some(Duration::MAX);
	}
	input_idle()
}

#[allow(unsafe_code)]
fn session_locked() -> bool {
	use windows::{
		Win32::System::RemoteDesktop::{
			WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory,
			WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
		},
		core::PWSTR,
	};
	let mut buffer = PWSTR::null();
	let mut len = 0;
	// SAFETY: the call fills `buffer` with a WTS allocation of `len` bytes; it is read only
	// when large enough for WTSINFOEXW (unaligned, in case) and always freed.
	unsafe {
		let queried = WTSQuerySessionInformationW(
			Some(WTS_CURRENT_SERVER_HANDLE),
			WTS_CURRENT_SESSION,
			WTSSessionInfoEx,
			&mut buffer,
			&mut len,
		);
		if queried.is_err() || buffer.is_null() {
			return false;
		}
		let locked = len as usize >= size_of::<WTSINFOEXW>() && {
			let info = buffer.0.cast::<WTSINFOEXW>().read_unaligned();
			info.Level == 1
				&& info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK as i32
		};
		WTSFreeMemory(buffer.0.cast());
		locked
	}
}

#[allow(unsafe_code)]
fn input_idle() -> Option<Duration> {
	use windows::Win32::{
		System::SystemInformation::GetTickCount,
		UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
	};
	let mut info = LASTINPUTINFO {
		cbSize: size_of::<LASTINPUTINFO>() as u32,
		dwTime: 0,
	};
	// SAFETY: stack LASTINPUTINFO with cbSize set; failure leaves dwTime at zero.
	let now = unsafe {
		let _ = GetLastInputInfo(&mut info);
		GetTickCount()
	};
	if info.dwTime == 0 {
		return None;
	}
	// Both are 32-bit millisecond tick counts that wrap together every ~49.7 days.
	Some(Duration::from_millis(u64::from(
		now.wrapping_sub(info.dwTime),
	)))
}
