//! Native background blur; transparency itself remains owned by winit/eframe.
use std::sync::Arc;
use winit::window::Window;

mod dwm;

pub struct Blur {
	native_enabled: bool,
	decorated: bool,
	// Keep the native display and surface alive until the protocol objects are dropped.
	window: Arc<Window>,
}

impl Blur {
	/// Initialize once on the window thread, before rendering starts.
	pub fn new(window: Arc<Window>) -> Self {
		Self {
			native_enabled: false,
			decorated: window.is_decorated(),
			window,
		}
	}

	/// Linux Wayland: keep the blur inside a rounded window of `[width, height, radius]`
	/// surface-local pixels, or `None` for the whole surface. Applied by the next `set_enabled`.
	pub fn set_rounded_shape(&mut self, shape: Option<[i32; 3]>) {
		let _ = shape;
	}

	pub fn set_enabled(&mut self, enabled: bool) {
		let unchanged = enabled == self.native_enabled;
		let unchanged = unchanged && self.decorated == self.window.is_decorated();
		if unchanged {
			return;
		}
		self.native_enabled = enabled;
		{
			use winit::platform::windows::{BackdropType, WindowExtWindows};
			self.decorated = self.window.is_decorated();
			if let Err(error) = dwm::extend_frame(&self.window, enabled) {
				eprintln!("Window blur: {error}");
			}
			self.window.set_system_backdrop(if enabled {
				BackdropType::TransientWindow
			} else {
				BackdropType::None
			});
		}
	}
}
