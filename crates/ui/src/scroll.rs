use crate::design;
use egui::{AsIdSalt, IdSalt, Pos2, Rect, ScrollArea, Shape, Stroke, pos2};

/// How long one wheel notch takes to glide when smooth scrolling is on. egui's own
/// smoothing reaches 90% of a notch in 0.1 s, which reads as a jump; this matches the
/// Slint client's 180 ms wheel animation.
pub const WHEEL_GLIDE_SECS: f32 = 0.18;

/// Point deltas at least this long are wheel notches in disguise (egui uses the same cut).
const COARSE_POINT_DELTA: f32 = 8.0;

/// Apply once before rendering the app's scroll areas. Zoom gestures remain unchanged.
pub fn apply_preferences(ctx: &egui::Context, preferences: model::ReadingPreferences) {
	if !preferences.is_valid() {
		return;
	}
	let options = ctx.options(|options| options.input_options);
	let glide_id = egui::Id::unique("dtewcord-wheel-glide");
	let mut pending = ctx
		.data(|data| data.get_temp::<egui::Vec2>(glide_id))
		.unwrap_or_default();
	ctx.input_mut(|input| {
		let page_height = input.viewport_rect().height();
		let delta = if preferences.smooth_scrolling {
			let (coarse, fine) = split_wheel_delta(&input.raw.events, options, page_height);
			pending += coarse;
			let dt = input.stable_dt.clamp(1.0 / 240.0, 0.05);
			glide_step(&mut pending, dt) + fine
		} else {
			pending = egui::Vec2::ZERO;
			instant_wheel_delta(&input.raw.events, options, page_height)
		};
		input.smooth_scroll_delta = delta * (f32::from(preferences.scroll_speed_percent) / 100.0);
	});
	ctx.data_mut(|data| data.insert_temp(glide_id, pending));
	if pending != egui::Vec2::ZERO {
		ctx.request_repaint();
	}
}

/// Release part of the distance still owed by earlier wheel notches: fast at first, easing
/// out, about 95% of it within [`WHEEL_GLIDE_SECS`]. Notches that arrive mid-glide add to
/// what is owed, so a quick spin keeps one continuous motion.
pub(super) fn glide_step(pending: &mut egui::Vec2, dt: f32) -> egui::Vec2 {
	let share = egui::emath::exponential_smooth_factor(0.95, WHEEL_GLIDE_SECS, dt);
	let mut step = egui::Vec2::ZERO;
	for axis in 0..2 {
		let left = pending[axis] * (1.0 - share);
		if left.abs() < 0.5 {
			step[axis] = pending[axis];
			pending[axis] = 0.0;
		} else {
			step[axis] = pending[axis] - left;
			pending[axis] = left;
		}
	}
	step
}

/// This frame's wheel movement in points, split into coarse mouse wheel notches (which
/// glide) and fine trackpad or high resolution movement (already smooth, so applied as is).
pub(super) fn split_wheel_delta(
	events: &[egui::Event],
	options: egui::InputOptions,
	page_height: f32,
) -> (egui::Vec2, egui::Vec2) {
	wheel_deltas(events, options, page_height).fold(
		(egui::Vec2::ZERO, egui::Vec2::ZERO),
		|(coarse, fine), (unit, delta)| {
			let is_fine =
				unit == egui::MouseWheelUnit::Point && delta.length() < COARSE_POINT_DELTA;
			if is_fine {
				(coarse, fine + delta)
			} else {
				(coarse + delta, fine)
			}
		},
	)
}

pub(super) fn instant_wheel_delta(
	events: &[egui::Event],
	options: egui::InputOptions,
	page_height: f32,
) -> egui::Vec2 {
	wheel_deltas(events, options, page_height)
		.fold(egui::Vec2::ZERO, |total, (_, delta)| total + delta)
}

/// Each scroll wheel event of the frame in points, with its original unit. Zoom gestures
/// are left out.
fn wheel_deltas(
	events: &[egui::Event],
	options: egui::InputOptions,
	page_height: f32,
) -> impl Iterator<Item = (egui::MouseWheelUnit, egui::Vec2)> + '_ {
	events.iter().filter_map(move |event| {
		let egui::Event::MouseWheel {
			unit,
			delta,
			phase,
			modifiers,
			..
		} = event
		else {
			return None;
		};
		if *phase != egui::TouchPhase::Move || modifiers.matches_any(options.zoom_modifier) {
			return None;
		}
		let mut delta = match unit {
			egui::MouseWheelUnit::Point => *delta,
			egui::MouseWheelUnit::Line => options.line_scroll_speed * *delta,
			egui::MouseWheelUnit::Page => page_height * *delta,
		};
		let horizontal = modifiers.matches_any(options.horizontal_scroll_modifier);
		let vertical = modifiers.matches_any(options.vertical_scroll_modifier);
		if horizontal && !vertical {
			delta = egui::vec2(delta.x + delta.y, 0.0);
		}
		if !horizontal && vertical {
			delta = egui::vec2(0.0, delta.x + delta.y);
		}
		Some((*unit, delta))
	})
}
/// Chromium / Discord default: 3 wheel lines times 40 px. winit reports one notch as `LineDelta` 1.0.
pub const DISCORD_LINE_SCROLL_SPEED: f32 = 120.0;

/// Chromium's middle-click autoscroll shape (`autoscroll_controller.cc`): a dead zone, then
/// the full distance from the origin raised to a power. The dead zone is a gate, not subtracted.
pub const DEAD_ZONE: f32 = 15.0;
const CURVE: f32 = 2.2;
const GAIN: f32 = 0.11;
const CEILING: f32 = 48_000.0;

/// The middle button over one frame, delivered outside egui's pointer state.
///
/// egui starts a text selection on `any_pressed()` while a selectable label is hovered, and
/// keeps extending it while any button is down. Middle counts. The window layer never gives
/// egui the button; it arrives here instead.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Middle {
	/// Position of the frame's first press, if it pressed.
	pub pressed: Option<Pos2>,
	/// Down at the end of the frame.
	pub down: bool,
}

/// Mouse 4 / mouse 5 edge presses for this frame, delivered outside egui's pointer state.
///
/// Label selection uses `any_pressed()`, so these buttons must not enter egui.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct SidePress {
	pub back: bool,
	pub forward: bool,
}

#[derive(Clone, Copy, Default)]
enum Drive {
	#[default]
	Idle,
	Driving {
		aim: Aim,
		hold: Hold,
	},
}

#[derive(Clone, Copy)]
struct Aim {
	origin: Pos2,
	cursor: Pos2,
	target: egui::Id,
}

#[derive(Clone, Copy)]
enum Hold {
	Button { wandered: bool },
	Latched,
}

#[derive(Default)]
pub struct Session {
	drive: Drive,
	middle: Middle,
	frame: Option<u64>,
	bound: bool,
	last_offset: Option<(egui::Id, f32)>,
	ignore_press: bool,
}

/// Scroll speed in points per second for a cursor `offset` points from the drive origin.
/// Signed like `offset`. Dead zone is a gate, not subtracted.
pub fn speed(offset: f32) -> f32 {
	let distance = offset.abs();
	if distance <= DEAD_ZONE {
		return 0.0;
	}
	offset.signum() * (GAIN * distance.powf(CURVE)).min(CEILING)
}

impl Session {
	/// Feed the frame's middle button before any `bind`/`attach`. Never fed means never pressed.
	pub fn middle(&mut self, middle: Middle) {
		self.middle = middle;
	}

	/// True while a drive needs the cursor position, including outside the window.
	pub fn tracking(&self) -> bool {
		matches!(self.drive, Drive::Driving { .. })
	}

	pub fn holding(&self) -> bool {
		!matches!(self.drive, Drive::Idle)
	}

	pub fn bind(&mut self, ui: &egui::Ui, target: egui::Id, area: Rect) -> f32 {
		let frame = ui.ctx().cumulative_frame_nr();
		if self.frame != Some(frame) {
			self.frame = Some(frame);
			self.bound = false;
			self.ignore_press = self.step(ui);
		}
		if !self.ignore_press {
			self.try_start(ui, target, area);
		}
		match self.drive {
			Drive::Driving { aim, .. } if aim.target == target => {
				self.bound = true;
				let dt = ui.input(|input| input.stable_dt).min(0.05);
				-speed(aim.cursor.y - aim.origin.y) * dt
			}
			_ => 0.0,
		}
	}

	pub fn attach(
		&mut self,
		ui: &egui::Ui,
		salt: impl AsIdSalt + Copy,
		builder: ScrollArea,
	) -> ScrollArea {
		let builder = builder.id_salt(salt);
		let target = ui.make_persistent_id(IdSalt::new(salt));
		let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
		let delta = self.bind(ui, target, area);
		if delta == 0.0 {
			if !self.holding() {
				self.last_offset = None;
			}
			return builder;
		}
		let Some(state) = egui::scroll_area::State::load(ui.ctx(), target) else {
			return builder;
		};
		let next = (state.clamped_offset().y - delta).max(0.0);
		if (next - state.clamped_offset().y).abs() < f32::EPSILON {
			return builder;
		}
		if let Some((id, last)) = self.last_offset
			&& id == target
			&& clamped_away(last, state.clamped_offset().y, next)
		{
			return builder;
		}
		self.last_offset = Some((target, next));
		ui.ctx().request_repaint();
		builder.vertical_scroll_offset(next)
	}

	pub fn paint(&self, ctx: &egui::Context) {
		let origin = match self.drive {
			Drive::Driving { aim, .. } => aim.origin,
			Drive::Idle => return,
		};
		let colors = design::palette_for(ctx);
		let painter = ctx.layer_painter(egui::LayerId::new(
			egui::Order::Foreground,
			egui::Id::unique("dtewcord-autoscroll"),
		));
		painter.circle_filled(origin, 12.0, colors.raised);
		painter.circle_stroke(origin, 12.0, Stroke::new(1.0, colors.muted));
		let tip = 3.6;
		let gap = 1.6;
		painter.add(Shape::convex_polygon(
			vec![
				pos2(origin.x, origin.y - gap - tip),
				pos2(origin.x - tip, origin.y - gap),
				pos2(origin.x + tip, origin.y - gap),
			],
			colors.text,
			Stroke::NONE,
		));
		painter.add(Shape::convex_polygon(
			vec![
				pos2(origin.x, origin.y + gap + tip),
				pos2(origin.x - tip, origin.y + gap),
				pos2(origin.x + tip, origin.y + gap),
			],
			colors.text,
			Stroke::NONE,
		));
		ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
	}

	pub fn clear_if_unbound(&mut self, ctx: &egui::Context) {
		if self.frame != Some(ctx.cumulative_frame_nr()) || !self.bound {
			self.drive = Drive::Idle;
			self.last_offset = None;
		}
	}

	fn step(&mut self, ui: &egui::Ui) -> bool {
		let Drive::Driving { mut aim, hold } = self.drive else {
			return false;
		};
		let middle = std::mem::take(&mut self.middle);
		let (focused, egui_pressed, escape, wheel, hover) = ui.input(|input| {
			(
				input.focused,
				input.pointer.any_pressed(),
				input.key_pressed(egui::Key::Escape),
				input.smooth_scroll_delta() != egui::Vec2::ZERO,
				input.pointer.hover_pos(),
			)
		});
		aim.cursor = hover.unwrap_or(aim.cursor);

		if !focused || escape || wheel || egui_pressed {
			self.idle();
			return egui_pressed;
		}
		match hold {
			Hold::Button { wandered } => {
				let wandered = wandered || (aim.cursor.y - aim.origin.y).abs() > DEAD_ZONE;
				if middle.down {
					self.drive = Drive::Driving {
						aim,
						hold: Hold::Button { wandered },
					};
				} else if wandered {
					self.idle();
				} else {
					self.drive = Drive::Driving {
						aim,
						hold: Hold::Latched,
					};
				}
			}
			Hold::Latched => {
				if middle.pressed.is_some() {
					self.idle();
					return true;
				}
				self.drive = Drive::Driving {
					aim,
					hold: Hold::Latched,
				};
			}
		}
		false
	}

	fn idle(&mut self) {
		self.drive = Drive::Idle;
		self.last_offset = None;
	}

	fn try_start(&mut self, ui: &egui::Ui, target: egui::Id, area: Rect) {
		if !matches!(self.drive, Drive::Idle) {
			return;
		}
		let Some(pos) = self.middle.pressed.filter(|pos| area.contains(*pos)) else {
			return;
		};
		if ui.input(|input| input.pointer.any_down()) {
			return;
		}
		self.drive = Drive::Driving {
			aim: Aim {
				origin: pos,
				cursor: pos,
				target,
			},
			hold: Hold::Button { wandered: false },
		};
		self.bound = true;
	}
}

fn clamped_away(requested: f32, current: f32, next: f32) -> bool {
	(requested - current).abs() > 0.5 && (next - current).signum() == (requested - current).signum()
}

#[cfg(test)]
mod glide_tests {
	use super::*;

	fn wheel(unit: egui::MouseWheelUnit, y: f32) -> egui::Event {
		egui::Event::MouseWheel {
			unit,
			delta: egui::vec2(0.0, y),
			modifiers: egui::Modifiers::NONE,
			phase: egui::TouchPhase::Move,
			source: egui::MouseWheelSource::Unknown,
		}
	}

	#[test]
	fn a_wheel_notch_glides_the_whole_distance_over_the_glide_time() {
		let mut pending = egui::vec2(0.0, -DISCORD_LINE_SCROLL_SPEED);
		let dt = 1.0 / 60.0;
		let first = glide_step(&mut pending, dt);
		// Not a jump: the first frame moves well under half a notch.
		assert!(first.y < 0.0 && first.y.abs() < DISCORD_LINE_SCROLL_SPEED * 0.5);
		let mut moved = first.y;
		let mut frames = 1;
		while pending != egui::Vec2::ZERO {
			moved += glide_step(&mut pending, dt).y;
			frames += 1;
			assert!(frames < 120, "glide never settles");
		}
		assert!((moved + DISCORD_LINE_SCROLL_SPEED).abs() < 1e-3);
		// Settles within a few glide times, not instantly.
		assert!(frames as f32 * dt > WHEEL_GLIDE_SECS * 0.5);
		assert!((frames as f32 * dt) < WHEEL_GLIDE_SECS * 3.0);
	}

	#[test]
	fn trackpad_movement_skips_the_glide() {
		let options = egui::InputOptions::default();
		let (coarse, fine) = split_wheel_delta(
			&[
				wheel(egui::MouseWheelUnit::Point, 3.0),
				wheel(egui::MouseWheelUnit::Line, -1.0),
				wheel(egui::MouseWheelUnit::Point, 40.0),
			],
			options,
			600.0,
		);
		assert_eq!(fine, egui::vec2(0.0, 3.0));
		assert_eq!(coarse, egui::vec2(0.0, 40.0 - options.line_scroll_speed));
	}

	#[test]
	fn smooth_preference_spreads_a_notch_over_frames() {
		let ctx = egui::Context::default();
		let preferences = model::ReadingPreferences::default();
		assert!(preferences.smooth_scrolling);
		ctx.options_mut(|options| {
			options.input_options.line_scroll_speed = DISCORD_LINE_SCROLL_SPEED;
		});
		let mut deltas = Vec::new();
		for frame in 0..30 {
			let events = if frame == 0 {
				vec![wheel(egui::MouseWheelUnit::Line, -1.0)]
			} else {
				Vec::new()
			};
			let output = ctx.run_ui(
				egui::RawInput {
					time: Some(f64::from(frame) / 60.0),
					predicted_dt: 1.0 / 60.0,
					events,
					..Default::default()
				},
				|ui| {
					apply_preferences(ui.ctx(), preferences);
					deltas.push(ui.input(|input| input.smooth_scroll_delta().y));
				},
			);
			output.drop_without_applying_deltas();
		}
		let moving = deltas.iter().filter(|delta| **delta != 0.0).count();
		assert!(moving > 3, "{deltas:?}");
		assert!(
			deltas[0].abs() < DISCORD_LINE_SCROLL_SPEED * 0.5,
			"{deltas:?}"
		);
		let total: f32 = deltas.iter().sum();
		assert!(
			(total + DISCORD_LINE_SCROLL_SPEED).abs() < 1e-2,
			"{deltas:?}"
		);
	}
}
