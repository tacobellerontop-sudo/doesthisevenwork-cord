//! One running client per user session. A second launch asks the running client to show its
//! window and exits, instead of starting another tray-resident instance. macOS bundles are
//! unique through LaunchServices, so there the Dock's reopen request shows the window.

use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

#[cfg(not(feature = "development-data"))]
const NAME: &str = "io.github.tacobellerontop_sudo.dtewcord";
#[cfg(feature = "development-data")]
const NAME: &str = "io.github.tacobellerontop_sudo.dtewcord.development";

pub enum Launch {
	Primary(Instance),
	/// Another instance owns this session and was asked to show itself.
	Forwarded,
}

/// Owns this session's instance claim; pending show requests are read by the UI.
#[derive(Default)]
pub struct Instance {
	requested: Arc<AtomicBool>,
	event: Option<win::Event>,
}

/// Claims this session. A failed claim never blocks startup; it only allows duplicates.
pub fn claim() -> Launch {
	match win::claim() {
		Some(event) => Launch::Primary(Instance {
			event,
			..Instance::default()
		}),
		None => Launch::Forwarded,
	}
}

impl Instance {
	/// Starts answering later launches. `restore` runs off the UI thread before `wake`, for
	/// compositors that send no frames to a hidden window.
	pub fn listen(
		&mut self,
		#[allow(unused_variables)] window: Arc<winit::window::Window>,
		restore: impl Fn() + Send + Sync + 'static,
		wake: impl Fn() + Send + Sync + 'static,
	) {
		let requested = self.requested.clone();
		let request = move || {
			restore();
			requested.store(true, Ordering::Release);
			wake();
		};
		if let Some(event) = self.event.take() {
			event.listen(request);
		}
	}

	/// Whether another launch or a Dock reopen asked to show the window since the last call.
	pub fn take_show_request(&self) -> bool {
		self.requested.swap(false, Ordering::AcqRel)
	}

	/// Gives up the claim when shutdown begins. A shutdown that then hangs, such as a media
	/// driver's exit handler, would otherwise keep accepting and swallowing later launches.
	pub fn release(&mut self) {}
}

#[allow(unsafe_code)]
mod win {
	use windows::{
		Win32::{
			Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0},
			System::Threading::{
				CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
				WaitForSingleObject,
			},
			UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow},
		},
		core::HSTRING,
	};

	/// A named auto-reset event in this logon session; it disappears with its last handle.
	pub struct Event(HANDLE);
	// SAFETY: kernel event handles may be waited on and closed from any thread.
	unsafe impl Send for Event {}

	fn name() -> HSTRING {
		HSTRING::from(format!("Local\\{}.show", super::NAME))
	}

	/// `None` when another instance owns the event and was signalled.
	pub fn claim() -> Option<Option<Event>> {
		let name = name();
		// SAFETY: the name outlives the call; default security keeps the event per-session.
		let Ok(handle) = (unsafe { CreateEventW(None, false, false, &name) }) else {
			return Some(None);
		};
		// SAFETY: read immediately after the CreateEventW call that set it.
		if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
			return Some(Some(Event(handle)));
		}
		// SAFETY: closes only the duplicate handle opened above.
		let _ = unsafe { CloseHandle(handle) };
		// SAFETY: the name outlives the call and the returned handle is closed below.
		let Ok(existing) = (unsafe { OpenEventW(EVENT_MODIFY_STATE, false, &name) }) else {
			return Some(None);
		};
		// SAFETY: this launch was started by the user, so it may lend foreground rights to the
		// running instance before waking it; `existing` is a valid event handle.
		unsafe {
			let _ = AllowSetForegroundWindow(ASFW_ANY);
			let _ = SetEvent(existing);
			let _ = CloseHandle(existing);
		}
		None
	}

	impl Event {
		fn wait(&self) -> bool {
			// SAFETY: the handle stays open for the process lifetime of the waiting thread.
			unsafe { WaitForSingleObject(self.0, INFINITE) == WAIT_OBJECT_0 }
		}
		pub fn listen(self, request: impl Fn() + Send + 'static) {
			let _ = std::thread::Builder::new()
				.name("dtewcord-instance".into())
				.spawn(move || {
					while self.wait() {
						request();
					}
				});
		}
	}
}
