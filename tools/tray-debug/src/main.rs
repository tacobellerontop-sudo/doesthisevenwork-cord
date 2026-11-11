//! Offline check of the tray close policy. Run with `cargo run --locked -p tray-debug`.

pub use egui;
// `tray_window.rs` addresses the compositor helper through `platform::compositor`, so this
// tool depends on the real crate; it's already built for other workspace members.
use platform as _;
#[path = "../../../apps/desktop/src/tray_window.rs"]
mod tray_window;

fn main() {
	check_window();
}

/// Exercise the desktop's actual close policy without creating a window or account.
fn check_window() {
	use egui::{ViewportCommand as Cmd, ViewportId};
	let ctx = egui::Context::default();
	let mut state = tray_window::State::default();
	let _restore = state.restorer();
	let mut close = egui::RawInput::default();
	close
		.viewports
		.get_mut(&ViewportId::ROOT)
		.unwrap()
		.events
		.push(egui::ViewportEvent::Close);
	for available in [false, true] {
		let _ = ctx.run_logic(&close, |ctx| {
			state.logic(ctx, available, true, Some(true));
			assert!(ctx.input(|i| i.viewport().close_requested()));
		});
	}
	state.cancel_quit();
	for can_hide in [true, false] {
		// Each case starts from a mapped window.
		state.show(&ctx);
		let output = ctx.run_logic(&close, |ctx| {
			state.logic(ctx, true, can_hide, Some(true));
			assert!(!ctx.input(|i| i.viewport().close_requested()));
			assert!(
				!state.hidden,
				"the hide stays pending until it is confirmed"
			);
		});
		let cmds = &output.viewport_commands[&ViewportId::ROOT];
		assert!(cmds.contains(&Cmd::CancelClose));
		assert!(cmds.contains(&if can_hide {
			Cmd::Visible(false)
		} else {
			Cmd::Minimized(true)
		}));
		// The compositor applies the request: unmapped for a winit hide, minimized otherwise.
		let mut confirmed = egui::RawInput::default();
		if !can_hide {
			confirmed
				.viewports
				.get_mut(&ViewportId::ROOT)
				.unwrap()
				.minimized = Some(true);
		}
		let output = ctx.run_logic(&confirmed, |ctx| {
			state.logic(ctx, true, can_hide, can_hide.then_some(false));
		});
		assert!(
			state.hidden,
			"a confirmed hide parks the window in the tray"
		);
		assert!(
			output
				.viewport_commands
				.get(&ViewportId::ROOT)
				.is_none_or(|cmds| !cmds.contains(&Cmd::Close)),
			"a confirmed hide must not also close"
		);
	}
	// Nobody can close the hidden window, so this close is the session ending: exit, unconfirmed,
	// within the logic pass, since a hidden window gets no ui pass to send a later close.
	let output = ctx.run_logic(&close, |ctx| {
		state.logic(ctx, true, true, Some(false));
		assert!(ctx.input(|i| i.viewport().close_requested()));
	});
	assert!(
		output
			.viewport_commands
			.get(&ViewportId::ROOT)
			.is_none_or(|cmds| !cmds.contains(&Cmd::CancelClose)),
		"the session close must not wait for the window to be shown"
	);
	assert!(state.system_close());
	state.show(&ctx);
	state.cancel_quit();
	assert!(!state.system_close());
	let _ = ctx.run_logic(&close, |ctx| state.logic(ctx, true, true, Some(true)));
	let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
		state.logic(ctx, true, true, Some(false))
	});
	assert!(state.hidden);
	let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
		state.logic(ctx, false, true, Some(true))
	});
	assert!(!state.hidden);
	assert!(output.viewport_commands[&ViewportId::ROOT].contains(&Cmd::Visible(true)));
	let output = ctx.run_logic(&close, |ctx| {
		state.quit(ctx);
		state.logic(ctx, true, true, Some(true));
	});
	assert!(!output.viewport_commands[&ViewportId::ROOT].contains(&Cmd::Close));
	let mut output = ctx.run_ui(egui::RawInput::default(), |ui| state.ui(ui.ctx()));
	output.textures_delta.clear();
	assert!(
		output.viewport_output[&ViewportId::ROOT]
			.commands
			.contains(&Cmd::Close)
	);
	let _ = ctx.run_logic(&close, |ctx| {
		state.logic(ctx, true, true, Some(true));
		assert!(ctx.input(|i| i.viewport().close_requested()));
	});
	state.cancel_quit();
	let _ = ctx.run_logic(&close, |ctx| state.logic(ctx, true, true, Some(true)));
	let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
		state.logic(ctx, true, true, Some(false))
	});
	assert!(state.hidden);

	// A compositor that ignores the hide must not strand an unclosable window: once the
	// confirmation budget runs out the app closes, through a close request the desktop's
	// own exit checks still see and can cancel.
	state.show(&ctx);
	let _ = ctx.run_logic(&close, |ctx| state.logic(ctx, true, true, Some(true)));
	let mut closed = false;
	for _ in 0..10 {
		let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
			state.logic(ctx, true, true, Some(true))
		});
		if output
			.viewport_commands
			.get(&ViewportId::ROOT)
			.is_some_and(|cmds| cmds.contains(&Cmd::Close))
		{
			closed = true;
			break;
		}
	}
	assert!(
		closed,
		"an ignored hide closes instead of stranding the window"
	);
	assert!(
		!state.hidden,
		"a failed hide never claims the window is hidden"
	);

	// Wayland reports neither visibility nor minimization, so an unconfirmable hide keeps
	// the previous tray behavior and never closes the app.
	state.cancel_quit();
	state.show(&ctx);
	let _ = ctx.run_logic(&close, |ctx| state.logic(ctx, true, false, None));
	let mut wayland_closed = false;
	for _ in 0..10 {
		let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
			state.logic(ctx, true, false, None);
		});
		wayland_closed |= output
			.viewport_commands
			.get(&ViewportId::ROOT)
			.is_some_and(|cmds| cmds.contains(&Cmd::Close));
	}
	assert!(
		!wayland_closed,
		"an unobservable Wayland hide never closes the app"
	);
	assert!(
		!state.hidden,
		"an unobservable hide is not claimed as hidden"
	);
	println!(
		"PASS: close/hide, hide confirmation, ignored-hide fallback, unobservable Wayland hide, hidden-window session close, host loss, deferred Quit and cancelled Quit."
	);
}
