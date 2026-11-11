//! Native global voice bindings, using the desktop portal on Wayland.
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use model::{KeyChord, KeybindAction, Keybinds, keybinds::is_mouse_button};
use std::sync::{Arc, Mutex, Weak};

const READY: &str = "Global voice keybinds are enabled.";
const DISABLED: &str = "Global keybinds are off. Shortcuts work while DTEW Cord is focused.";
const UNAVAILABLE: &str =
	"Global voice keybinds are unavailable on this system; they work while DTEW Cord is focused.";
const INVALID: &str = "One or more voice bindings cannot be registered globally; they still work while DTEW Cord is focused.";
const MODIFIER_REQUIRED: &str = "Add Ctrl, Alt, Shift, or Command to use a voice binding globally; it still works while DTEW Cord is focused.";

/// Mouse poller identities, kept out of the range `global-hotkey` assigns to real hotkeys.
const MOUSE_ID: u32 = 1 << 31;

static NATIVE_INPUTS: Mutex<Vec<Weak<NativeInput>>> = Mutex::new(Vec::new());

#[derive(Default)]
struct NativeState {
	/// Registered native hotkeys and the action each one triggers.
	registered: Vec<(u32, KeybindAction)>,
	/// Push-to-talk/mute keys currently held, so releasing one key keeps the others down.
	held: Vec<(u32, KeybindAction)>,
	pending_toggles: u8,
}

struct NativeInput {
	state: Mutex<NativeState>,
	wake: Arc<dyn Fn() + Send + Sync>,
}

impl NativeInput {
	fn handle(&self, event: GlobalHotKeyEvent) {
		let action = self
			.state
			.lock()
			.expect("native hotkey state poisoned")
			.registered
			.iter()
			.find(|(id, _)| *id == event.id())
			.map(|(_, action)| *action);
		if let Some(action) = action {
			self.apply(event.id(), action, event.state() == HotKeyState::Pressed);
		}
	}

	/// Folds one press or release of key `id` and wakes the host.
	fn apply(&self, id: u32, action: KeybindAction, pressed: bool) {
		let mut state = self.state.lock().expect("native hotkey state poisoned");
		let handled = match (action, pressed) {
			(KeybindAction::PushToTalk | KeybindAction::PushToMute, _) => {
				state.held.retain(|(held, _)| *held != id);
				if pressed {
					state.held.push((id, action));
				}
				true
			}
			(KeybindAction::ToggleMute, true) => {
				state.pending_toggles ^= 1;
				true
			}
			(KeybindAction::ToggleDeafen, true) => {
				state.pending_toggles ^= 2;
				true
			}
			_ => false,
		};
		drop(state);
		if handled {
			(self.wake)();
		}
	}

	fn clear(&self) {
		*self.state.lock().expect("native hotkey state poisoned") = NativeState::default();
	}

	fn register(&self, id: u32, action: KeybindAction) {
		self.state
			.lock()
			.expect("native hotkey state poisoned")
			.registered
			.push((id, action));
	}

	fn take_toggles(&self) -> u8 {
		std::mem::take(
			&mut self
				.state
				.lock()
				.expect("native hotkey state poisoned")
				.pending_toggles,
		)
	}

	/// Whether any key currently held triggers `action`.
	fn push_down(&self, action: KeybindAction) -> bool {
		self.state
			.lock()
			.expect("native hotkey state poisoned")
			.held
			.iter()
			.any(|(_, held)| *held == action)
	}
}

fn dispatch_native_event(event: GlobalHotKeyEvent) {
	let inputs = NATIVE_INPUTS
		.lock()
		.expect("native hotkey targets poisoned")
		.iter()
		.filter_map(Weak::upgrade)
		.collect::<Vec<_>>();
	for input in inputs {
		input.handle(event);
	}
}

pub struct Hotkeys {
	manager: Option<GlobalHotKeyManager>,
	/// Registered native hotkeys and the chord/action each one owns.
	registered: Vec<(HotKey, KeybindAction, KeyChord)>,
	bindings: Option<Vec<(KeybindAction, KeyChord)>>,
	native: Arc<NativeInput>,
	status: &'static str,
	mouse: Option<mouse::Poller>,
}

impl Hotkeys {
	pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
		let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
		let native = Arc::new(NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: wake.clone(),
		});
		NATIVE_INPUTS
			.lock()
			.expect("native hotkey targets poisoned")
			.push(Arc::downgrade(&native));
		GlobalHotKeyEvent::set_event_handler(Some(dispatch_native_event));
		let manager = GlobalHotKeyManager::new().ok();
		let status = if manager.is_some() {
			READY
		} else {
			UNAVAILABLE
		};
		Self {
			manager,
			registered: Vec::new(),
			bindings: None,
			native,
			status,
			mouse: None,
		}
	}

	pub fn sync(&mut self, keybinds: &Keybinds, _runtime: &tokio::runtime::Runtime) {
		let next = keybinds.global_enabled.then(|| {
			KeybindAction::ALL
				.into_iter()
				.filter(|action| action.is_global())
				.flat_map(|action| {
					keybinds
						.chords(action)
						.cloned()
						.map(move |chord| (action, chord))
				})
				.collect::<Vec<_>>()
		});
		if self.bindings == next {
			return;
		}
		self.bindings = next.clone();
		self.unregister_all();

		let Some(next) = next else {
			return;
		};

		let Some(manager) = &self.manager else {
			return;
		};
		let mut failed = false;
		let mut modifier_required = false;
		for (action, chord) in &next {
			// An unassigned binding, such as the default push-to-mute row, is simply absent.
			if chord.key.is_empty() {
				continue;
			}
			if !chord.is_valid() {
				failed = true;
				continue;
			}
			if is_mouse_button(&chord.key) {
				// Windows polls mouse bindings below; elsewhere they stay focused-only.
				failed |= false;
				continue;
			}
			if chord.modifiers == 0 && !is_standalone_global_key(&chord.key) {
				modifier_required |= !matches!(
					action,
					KeybindAction::PushToTalk | KeybindAction::PushToMute
				);
				continue;
			}
			let Some(hotkey) = native_hotkey(chord) else {
				failed = true;
				continue;
			};
			// Cmd/Ctrl variants can translate to the same native hotkey. A same-action alias
			// is redundant, while a collision with another action leaves that action's
			// chord focused-only and reports the configuration as invalid.
			if let Some((_, registered, _)) = self
				.registered
				.iter()
				.find(|(existing, ..)| existing == &hotkey)
			{
				failed |= *registered != *action;
				continue;
			}
			match manager.register(hotkey) {
				Ok(()) => {
					self.native.register(hotkey.id(), *action);
					self.registered.push((hotkey, *action, chord.clone()));
				}
				Err(_) => failed = true,
			}
		}
		{
			self.mouse = mouse::Poller::start(&next, self.native.clone());
		}
		if failed {
			self.status = INVALID;
		} else if modifier_required {
			self.status = MODIFIER_REQUIRED;
		} else {
			self.status = READY;
		}
	}

	fn unregister_all(&mut self) {
		// Stop the poller first so it cannot publish a stale press after the clear.
		drop(self.mouse.take());
		self.native.clear();
		if let Some(manager) = &self.manager {
			for (hotkey, ..) in self.registered.drain(..) {
				let _ = manager.unregister(hotkey);
			}
		} else {
			self.registered.clear();
		}
	}

	pub fn take_toggle_pending(&mut self) -> u8 {
		self.native.take_toggles()
	}

	/// Whether this exact chord is owned by a native global registration.
	pub fn is_global(&self, action: KeybindAction, chord: &KeyChord) -> bool {
		if self
			.registered
			.iter()
			.any(|(_, registered, current)| *registered == action && current == chord)
		{
			return true;
		}
		if let Some(poller) = &self.mouse
			&& poller.registers(action, chord)
		{
			return true;
		}
		false
	}

	/// Bits for mute/deafen bindings currently owned by the native global registrar.
	pub fn global_toggle_mask(&self) -> u8 {
		let mut mask = self.registered.iter().fold(0, |mask, (_, action, _)| {
			mask | match action {
				KeybindAction::ToggleMute => 1,
				KeybindAction::ToggleDeafen => 2,
				_ => 0,
			}
		});
		if let Some(poller) = &self.mouse {
			mask |= poller.toggle_mask();
		}
		mask
	}

	pub fn push_to_talk_down(&self) -> bool {
		self.native.push_down(KeybindAction::PushToTalk)
	}

	pub fn push_to_mute_down(&self) -> bool {
		self.native.push_down(KeybindAction::PushToMute)
	}

	pub fn status(&self) -> &'static str {
		if self.bindings.is_none() {
			return DISABLED;
		}
		self.status
	}
}

impl Drop for Hotkeys {
	fn drop(&mut self) {
		self.unregister_all();
		let native = Arc::downgrade(&self.native);
		NATIVE_INPUTS
			.lock()
			.expect("native hotkey targets poisoned")
			.retain(|current| !current.ptr_eq(&native));
	}
}

fn is_standalone_global_key(name: &str) -> bool {
	matches!(
		name,
		"PageDown"
			| "PageUp"
			| "Insert"
			| "F1" | "F2"
			| "F3" | "F4"
			| "F5" | "F6"
			| "F7" | "F8"
			| "F9" | "F10"
			| "F11" | "F12"
			| "F13" | "F14"
			| "F15" | "F16"
			| "F17" | "F18"
			| "F19" | "F20"
			| "F21" | "F22"
			| "F23" | "F24"
	)
}

fn native_hotkey(chord: &KeyChord) -> Option<HotKey> {
	if !chord.is_valid() || (chord.modifiers == 0 && !is_standalone_global_key(&chord.key)) {
		return None;
	}
	let mut value = String::new();
	if chord.modifiers & model::keybinds::PRIMARY != 0 {
		value.push_str("control+");
	}
	if chord.modifiers & model::keybinds::CTRL != 0 {
		value.push_str("control+");
	}
	if chord.modifiers & model::keybinds::ALT != 0 {
		value.push_str("alt+");
	}
	if chord.modifiers & model::keybinds::SHIFT != 0 {
		value.push_str("shift+");
	}
	value.push_str(code_name(&chord.key)?);
	value.parse().ok()
}

fn code_name(name: &str) -> Option<&'static str> {
	match name {
		"ArrowDown" => Some("ArrowDown"),
		"ArrowLeft" => Some("ArrowLeft"),
		"ArrowRight" => Some("ArrowRight"),
		"ArrowUp" => Some("ArrowUp"),
		"Escape" => Some("Escape"),
		"Tab" => Some("Tab"),
		"Backspace" => Some("Backspace"),
		"Enter" => Some("Enter"),
		"Space" => Some("Space"),
		"Delete" => Some("Delete"),
		"Home" => Some("Home"),
		"End" => Some("End"),
		"PageDown" => Some("PageDown"),
		"PageUp" => Some("PageUp"),
		"Insert" => Some("Insert"),
		"Slash" => Some("Slash"),
		"Backtick" => Some("Backquote"),
		"Minus" => Some("Minus"),
		"Equals" => Some("Equal"),
		"Comma" => Some("Comma"),
		"Period" => Some("Period"),
		"OpenBracket" | "OpenCurlyBracket" => Some("BracketLeft"),
		"CloseBracket" | "CloseCurlyBracket" => Some("BracketRight"),
		"Backslash" | "IntlBackslash" => Some("Backslash"),
		"Semicolon" | "Colon" => Some("Semicolon"),
		"Quote" => Some("Quote"),
		"Plus" => Some("Equal"),
		"Pipe" => Some("Backslash"),
		"Questionmark" => Some("Slash"),
		"Exclamationmark" => Some("Digit1"),
		"Num0" => Some("Digit0"),
		"Num1" => Some("Digit1"),
		"Num2" => Some("Digit2"),
		"Num3" => Some("Digit3"),
		"Num4" => Some("Digit4"),
		"Num5" => Some("Digit5"),
		"Num6" => Some("Digit6"),
		"Num7" => Some("Digit7"),
		"Num8" => Some("Digit8"),
		"Num9" => Some("Digit9"),
		"A" => Some("KeyA"),
		"B" => Some("KeyB"),
		"C" => Some("KeyC"),
		"D" => Some("KeyD"),
		"E" => Some("KeyE"),
		"F" => Some("KeyF"),
		"G" => Some("KeyG"),
		"H" => Some("KeyH"),
		"I" => Some("KeyI"),
		"J" => Some("KeyJ"),
		"K" => Some("KeyK"),
		"L" => Some("KeyL"),
		"M" => Some("KeyM"),
		"N" => Some("KeyN"),
		"O" => Some("KeyO"),
		"P" => Some("KeyP"),
		"Q" => Some("KeyQ"),
		"R" => Some("KeyR"),
		"S" => Some("KeyS"),
		"T" => Some("KeyT"),
		"U" => Some("KeyU"),
		"V" => Some("KeyV"),
		"W" => Some("KeyW"),
		"X" => Some("KeyX"),
		"Y" => Some("KeyY"),
		"Z" => Some("KeyZ"),
		"F1" => Some("F1"),
		"F2" => Some("F2"),
		"F3" => Some("F3"),
		"F4" => Some("F4"),
		"F5" => Some("F5"),
		"F6" => Some("F6"),
		"F7" => Some("F7"),
		"F8" => Some("F8"),
		"F9" => Some("F9"),
		"F10" => Some("F10"),
		"F11" => Some("F11"),
		"F12" => Some("F12"),
		"F13" => Some("F13"),
		"F14" => Some("F14"),
		"F15" => Some("F15"),
		"F16" => Some("F16"),
		"F17" => Some("F17"),
		"F18" => Some("F18"),
		"F19" => Some("F19"),
		"F20" => Some("F20"),
		"F21" => Some("F21"),
		"F22" => Some("F22"),
		"F23" => Some("F23"),
		"F24" => Some("F24"),
		_ => None,
	}
}

/// Windows has no global mouse-button hotkeys, so the physical button state is polled instead.
mod mouse {
	use super::{KeyChord, KeybindAction, MOUSE_ID, NativeInput};
	use model::keybinds::{ALT, CTRL, PRIMARY, SHIFT};
	use std::sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	};
	use std::time::Duration;
	use windows::Win32::UI::Input::KeyboardAndMouse::{
		GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MBUTTON, VK_MENU, VK_SHIFT, VK_XBUTTON1,
		VK_XBUTTON2,
	};

	/// Well under an ordinary click, which stays down for tens of milliseconds.
	const PERIOD: Duration = Duration::from_millis(10);

	/// Runs only while a global voice action is bound to a mouse button.
	pub(super) struct Poller {
		stop: Arc<AtomicBool>,
		thread: Option<std::thread::JoinHandle<()>>,
		toggles: u8,
		chords: Vec<(KeybindAction, KeyChord)>,
	}

	impl Poller {
		pub(super) fn start(
			bindings: &[(KeybindAction, KeyChord)],
			native: Arc<NativeInput>,
		) -> Option<Self> {
			let watched: Vec<_> = bindings
				.iter()
				.filter_map(|(action, chord)| Some((*action, button(&chord.key)?, chord.modifiers)))
				.collect();
			if watched.is_empty() {
				return None;
			}
			let toggles = watched.iter().fold(0, |mask, (action, ..)| {
				mask | match action {
					KeybindAction::ToggleMute => 1,
					KeybindAction::ToggleDeafen => 2,
					_ => 0,
				}
			});
			// Only mouse chords are polled, so keyboard chords never count as global here.
			let chords: Vec<_> = bindings
				.iter()
				.filter(|(_, chord)| button(&chord.key).is_some())
				.cloned()
				.collect();
			// One group per action and physical button; modifier variants within a group OR.
			let mut toggle_groups: Vec<(KeybindAction, VIRTUAL_KEY)> = Vec::new();
			for (action, button, _) in &watched {
				if matches!(
					action,
					KeybindAction::ToggleMute | KeybindAction::ToggleDeafen
				) && !toggle_groups.iter().any(|(group_action, group_button)| {
					group_action == action && group_button.0 == button.0
				}) {
					toggle_groups.push((*action, *button));
				}
			}
			let stop = Arc::new(AtomicBool::new(false));
			let stopped = stop.clone();
			let thread = std::thread::Builder::new()
				.name("dtewcord-mouse-keybinds".into())
				.spawn(move || {
					let mut held = vec![false; watched.len()];
					let mut held_toggles = vec![false; toggle_groups.len()];
					while !stopped.load(Ordering::Relaxed) {
						let next: Vec<bool> = watched
							.iter()
							.map(|(_, button, modifiers)| {
								key_down(*button) && modifiers_match(*modifiers)
							})
							.collect();
						for (index, ((action, ..), (was, now))) in
							watched.iter().zip(held.iter().zip(&next)).enumerate()
						{
							// Toggle chords fold into one transition per button below.
							if was != now
								&& !matches!(
									action,
									KeybindAction::ToggleMute | KeybindAction::ToggleDeafen
								) {
								native.apply(MOUSE_ID + index as u32, *action, *now);
							}
						}
						for (group, (action, button)) in toggle_groups.iter().enumerate() {
							let down = watched.iter().zip(&next).any(
								|((watched, watched_button, _), down)| {
									watched == action && watched_button.0 == button.0 && *down
								},
							);
							if down != held_toggles[group] {
								held_toggles[group] = down;
								native.apply(MOUSE_ID + 64 + group as u32, *action, down);
							}
						}
						held = next;
						std::thread::sleep(PERIOD);
					}
				})
				.ok()?;
			Some(Self {
				stop,
				thread: Some(thread),
				toggles,
				chords,
			})
		}

		/// Mute/deafen bits this poller owns, so focused input does not toggle them twice.
		pub(super) fn toggle_mask(&self) -> u8 {
			self.toggles
		}

		/// Whether this poller watches the exact chord.
		pub(super) fn registers(&self, action: KeybindAction, chord: &KeyChord) -> bool {
			self.chords
				.iter()
				.any(|(watched, current)| *watched == action && current == chord)
		}
	}

	impl Drop for Poller {
		fn drop(&mut self) {
			self.stop.store(true, Ordering::Relaxed);
			if let Some(thread) = self.thread.take() {
				let _ = thread.join();
			}
		}
	}

	fn button(name: &str) -> Option<VIRTUAL_KEY> {
		match name {
			"MouseMiddle" => Some(VK_MBUTTON),
			"MouseExtra1" => Some(VK_XBUTTON1),
			"MouseExtra2" => Some(VK_XBUTTON2),
			_ => None,
		}
	}

	/// Mirrors egui's `matches_logically` for focused input: extra Shift or Alt still
	/// match, while Ctrl must agree so Ctrl+button stays a distinct binding.
	fn modifiers_match(modifiers: u8) -> bool {
		key_down(VK_CONTROL) == (modifiers & (PRIMARY | CTRL) != 0)
			&& (modifiers & SHIFT == 0 || key_down(VK_SHIFT))
			&& (modifiers & ALT == 0 || key_down(VK_MENU))
	}

	#[allow(unsafe_code)]
	fn key_down(key: VIRTUAL_KEY) -> bool {
		// SAFETY: GetAsyncKeyState only reads global input state for a virtual-key code.
		unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicBool, Ordering};

	#[test]
	fn modifier_bindings_have_native_codes_and_plain_keys_stay_focused() {
		for key in [
			"OpenBracket",
			"CloseBracket",
			"OpenCurlyBracket",
			"CloseCurlyBracket",
			"Backslash",
			"IntlBackslash",
			"Semicolon",
			"Colon",
			"Quote",
			"Plus",
			"Pipe",
			"Questionmark",
			"Exclamationmark",
		] {
			assert!(
				native_hotkey(&KeyChord::new(key, model::keybinds::PRIMARY)).is_some(),
				"native hotkey mapping missing for {key}"
			);
		}
		assert!(
			native_hotkey(&KeyChord::new(
				"M",
				model::keybinds::PRIMARY | model::keybinds::SHIFT
			))
			.is_some()
		);
		assert!(native_hotkey(&KeyChord::default()).is_none());
		assert!(native_hotkey(&KeyChord::new("unknown", model::keybinds::PRIMARY)).is_none());
		// Plain letter key stays focused-only so typing is not swallowed
		assert!(native_hotkey(&KeyChord::new("M", 0)).is_none());
		// Standalone navigation and function keys can be registered globally
		assert!(native_hotkey(&KeyChord::new("PageDown", 0)).is_some());
		assert!(native_hotkey(&KeyChord::new("PageUp", 0)).is_some());
		assert!(native_hotkey(&KeyChord::new("Insert", 0)).is_some());
		assert!(native_hotkey(&KeyChord::new("F12", 0)).is_some());
		assert!(native_hotkey(&KeyChord::new("F13", 0)).is_some());
		assert!(native_hotkey(&KeyChord::new("F24", 0)).is_some());
		// Mouse buttons have no native hotkey code.
		assert!(native_hotkey(&KeyChord::new("MouseExtra1", 0)).is_none());
	}

	#[test]
	fn push_to_mute_follows_press_and_release() {
		let input = NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: Arc::new(|| {}),
		};
		input.register(7, KeybindAction::PushToMute);
		input.handle(GlobalHotKeyEvent {
			id: 7,
			state: HotKeyState::Pressed,
		});
		assert!(input.push_down(KeybindAction::PushToMute));
		assert!(!input.push_down(KeybindAction::PushToTalk));
		input.handle(GlobalHotKeyEvent {
			id: 7,
			state: HotKeyState::Released,
		});
		assert!(!input.push_down(KeybindAction::PushToMute));
		assert_eq!(input.take_toggles(), 0);
	}

	#[test]
	fn each_push_key_releases_without_clearing_the_other() {
		let input = NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: Arc::new(|| {}),
		};
		input.register(3, KeybindAction::PushToTalk);
		input.register(5, KeybindAction::PushToTalk);
		for id in [3, 5] {
			input.handle(GlobalHotKeyEvent {
				id,
				state: HotKeyState::Pressed,
			});
		}
		assert!(input.push_down(KeybindAction::PushToTalk));
		input.handle(GlobalHotKeyEvent {
			id: 3,
			state: HotKeyState::Released,
		});
		assert!(input.push_down(KeybindAction::PushToTalk));
		input.handle(GlobalHotKeyEvent {
			id: 5,
			state: HotKeyState::Released,
		});
		assert!(!input.push_down(KeybindAction::PushToTalk));
	}

	#[test]
	fn native_events_wake_and_fold_without_a_queue() {
		let woke = Arc::new(AtomicBool::new(false));
		let wake_flag = woke.clone();
		let input = NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: Arc::new(move || {
				wake_flag.store(true, Ordering::Relaxed);
			}),
		};
		input.register(42, KeybindAction::ToggleMute);
		input.handle(GlobalHotKeyEvent {
			id: 42,
			state: HotKeyState::Pressed,
		});
		assert_eq!(input.take_toggles(), 1);
		assert!(woke.load(Ordering::Relaxed));
	}

	#[test]
	fn native_events_reach_each_live_matching_input() {
		let first = Arc::new(NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: Arc::new(|| {}),
		});
		let second = Arc::new(NativeInput {
			state: Mutex::new(NativeState::default()),
			wake: Arc::new(|| {}),
		});
		first.register(42, KeybindAction::ToggleMute);
		second.register(84, KeybindAction::ToggleDeafen);
		{
			let mut inputs = NATIVE_INPUTS
				.lock()
				.expect("native hotkey targets poisoned");
			inputs.clear();
			inputs.extend([Arc::downgrade(&first), Arc::downgrade(&second)]);
		}

		dispatch_native_event(GlobalHotKeyEvent {
			id: 42,
			state: HotKeyState::Pressed,
		});
		dispatch_native_event(GlobalHotKeyEvent {
			id: 84,
			state: HotKeyState::Pressed,
		});

		assert_eq!(first.take_toggles(), 1);
		assert_eq!(second.take_toggles(), 2);
		NATIVE_INPUTS
			.lock()
			.expect("native hotkey targets poisoned")
			.clear();
	}
}
