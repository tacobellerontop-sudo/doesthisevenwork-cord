//! Compositor-level window hiding for Wayland sessions where winit cannot hide or minimize.
//!
//! Hyprland has no minimize concept and ignores `xdg_toplevel.set_minimized`, and winit's Wayland
//! backend cannot unmap a surface. Its IPC socket can still park this process's windows on a
//! named special workspace, which is the established Hyprland way to hide a window to the tray.
//! KWin minimizes, but keeps the taskbar entry; its scripting interface can skip the taskbar.

/// Commands the running compositor to hide or restore this process's windows.
#[derive(Clone, Debug)]
pub struct Hider {}

impl Hider {
	/// Other platforms hide windows through winit, so nothing is detected here.
	pub fn detect() -> Option<Self> {
		None
	}
	pub fn available(&self) -> bool {
		false
	}
	pub fn hide(&self) {}
	pub fn show(&self) {}
	pub fn minimize(&self) -> bool {
		false
	}
}
