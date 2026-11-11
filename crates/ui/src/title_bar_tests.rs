use super::*;
use egui::{Event as InputEvent, PointerButton, ViewportCommand};

fn frame_with_maximized(
	ctx: &egui::Context,
	view: &mut MessagingUi,
	width: f32,
	events: Vec<InputEvent>,
	maximized: Option<bool>,
) -> Vec<ViewportCommand> {
	let mut raw = egui::RawInput {
		screen_rect: Some(egui::Rect::from_min_size(
			egui::Pos2::ZERO,
			egui::vec2(width, 600.0),
		)),
		focused: true,
		events,
		..Default::default()
	};
	if let Some(maximized) = maximized {
		raw.viewports
			.entry(egui::ViewportId::ROOT)
			.or_default()
			.maximized = Some(maximized);
	}
	let output = ctx.run_ui(raw, |ui| {
		view.title_bar(
			ui,
			&mut State::default(),
			&mut Vec::new(),
			"Synthetic context title",
		);

		// The desktop shows resize handles alongside the strip; include them
		// so corner clicks are tested against the real hit-testing stack.
		crate::design::window_resize(ui.ctx());
	});
	let commands = output.viewport_output[&egui::ViewportId::ROOT]
		.commands
		.clone();
	output.drop_without_applying_deltas();
	commands
}

fn pointer(pos: egui::Pos2, button: PointerButton, pressed: bool) -> Vec<InputEvent> {
	vec![
		InputEvent::PointerMoved(pos),
		InputEvent::PointerButton {
			pos,
			button,
			pressed,
			modifiers: egui::Modifiers::NONE,
		},
	]
}

#[test]
fn hidden_title_strip_does_not_start_window_drag() {
	let ctx = egui::Context::default();
	let mut view = MessagingUi::default();
	let mut state = test_support::demo_state();
	let run = |view: &mut MessagingUi, state: &mut State, events| {
		let output = ctx.run_ui(
			egui::RawInput {
				screen_rect: Some(egui::Rect::from_min_size(
					egui::Pos2::ZERO,
					egui::vec2(1200.0, 760.0),
				)),
				focused: true,
				events,
				..Default::default()
			},
			|ui| {
				view.show(ui, state);
			},
		);
		let commands = output.viewport_output[&egui::ViewportId::ROOT]
			.commands
			.clone();
		output.drop_without_applying_deltas();
		commands
	};
	let pos = egui::pos2(600.0, 18.0);
	run(&mut view, &mut state, vec![]);
	assert_eq!(
		run(
			&mut view,
			&mut state,
			pointer(pos, PointerButton::Primary, true)
		),
		[ViewportCommand::StartDrag]
	);
	run(
		&mut view,
		&mut state,
		pointer(pos, PointerButton::Primary, false),
	);
	view.hide_title_bar = true;
	run(&mut view, &mut state, vec![]);
	assert!(
		!run(
			&mut view,
			&mut state,
			pointer(pos, PointerButton::Primary, true)
		)
		.contains(&ViewportCommand::StartDrag)
	);
}

#[test]
fn restore_caption_button_emits_unmaximize_when_maximized() {
	let ctx = egui::Context::default();
	let mut view = MessagingUi::default();
	let width = 760.0;
	frame_with_maximized(&ctx, &mut view, width, vec![], Some(true));
	let pos = egui::pos2(width - 69.0, 18.0);
	assert!(
		frame_with_maximized(
			&ctx,
			&mut view,
			width,
			pointer(pos, PointerButton::Primary, true),
			Some(true)
		)
		.is_empty()
	);
	assert_eq!(
		frame_with_maximized(
			&ctx,
			&mut view,
			width,
			pointer(pos, PointerButton::Primary, false),
			Some(true)
		),
		[ViewportCommand::Maximized(false)]
	);
}
