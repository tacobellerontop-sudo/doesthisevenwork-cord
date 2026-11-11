//! DTEW Cord's Slint client: the same sign-in, credential store, REST and Gateway crates as the
//! egui desktop app, with a new Slint presentation layer.
//!
//! `--demo` (with the `demo` feature) opens synthetic offline data and never contacts Discord.

mod clock;
mod markdown;
mod session;
mod view;

mod ui {
	#![allow(unsafe_code)]
	slint::include_modules!();
}

use client_core::{
	Command, Envelope, Event, State,
	auth::{AuthState, Failure, SessionSecret},
};
use model::{Delivery, Id};
use session::Session;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
	cell::RefCell,
	collections::BTreeSet,
	rc::Rc,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
	time::{Duration, Instant},
};
use ui::{AppWindow, ChannelItem, GuildItem, MessageItem, Theme};

/// Which screen `AppWindow.screen` shows.
mod screen {
	pub const SIGN_IN: i32 = 0;
	pub const CONNECTING: i32 = 1;
	pub const MESSAGING: i32 = 2;
}

fn rgb(value: u32) -> slint::Color {
	slint::Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// Where the sidebar points: Direct Messages or one server.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
	Home,
	Guild(Id),
}

struct App {
	ui: slint::Weak<AppWindow>,
	runtime: tokio::runtime::Runtime,
	clock: clock::Clock,
	state: State,
	session: Option<Session>,
	login: Option<platform::LoginView>,
	/// The secret of the running session and whether to save it once Discord accepts it.
	secret: Option<(Arc<SessionSecret>, bool)>,
	place: Place,
	collapsed: BTreeSet<Id>,
	opened: Option<Id>,
	serial: i32,
	sign_in_status: String,
	restoring: bool,
	wake: session::Wake,
	#[cfg_attr(not(feature = "demo"), allow(dead_code))]
	demo_ids: u64,
	dirty: bool,
}

thread_local! {
	static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
	APP.with(|slot| slot.borrow_mut().as_mut().map(f))
}

/// Wakes coalesce: many queued events cost one pump on the UI thread.
fn waker() -> session::Wake {
	let pending = Arc::new(AtomicBool::new(false));
	Arc::new(move || {
		if !pending.swap(true, Ordering::AcqRel) {
			let pending = pending.clone();
			let _ = slint::invoke_from_event_loop(move || {
				pending.store(false, Ordering::Release);
				with_app(App::pump);
			});
		}
	})
}

impl App {
	fn ui(&self) -> AppWindow {
		self.ui.upgrade().expect("the window outlives the app")
	}

	fn begin(&mut self, demo: bool) {
		if demo {
			#[cfg(feature = "demo")]
			{
				self.state = test_support::demo_state();
				if let Some(guild) = self
					.state
					.selected
					.and_then(|id| self.state.channel(id))
					.and_then(|c| c.guild)
				{
					self.place = Place::Guild(guild);
				}
				self.dirty = true;
				self.refresh();
				return;
			}
		}
		self.restoring = true;
		self.sign_in_status = "Looking for a saved login…".into();
		self.refresh();
		// Keychain reads can block on a desktop prompt; keep them off the UI thread.
		std::thread::spawn(|| {
			let loaded = platform::load_session();
			let _ = slint::invoke_from_event_loop(move || {
				with_app(|app| app.restored(loaded));
			});
		});
	}

	fn restored(&mut self, loaded: Result<Option<SessionSecret>, platform::CredentialError>) {
		match loaded {
			Ok(Some(secret)) => self.connect(secret, false),
			Ok(None) => {
				self.restoring = false;
				self.sign_in_status.clear();
			}
			Err(_) => {
				self.restoring = false;
				self.sign_in_status =
					"The system keychain is unavailable, so the login can't be restored or saved."
						.into();
			}
		}
		self.refresh();
	}

	fn connect(&mut self, secret: SessionSecret, save: bool) {
		self.session = None;
		self.login = None;
		self.state.generation = self.state.generation.wrapping_add(1);
		self.state.auth = AuthState::Authenticating;
		self.state.status = "Connecting to Discord…";
		self.state.failure_detail = None;
		let secret = Arc::new(secret);
		self.secret = Some((secret.clone(), save));
		self.session = Some(Session::start(
			self.runtime.handle(),
			secret,
			self.state.generation,
			self.state.user.as_ref().map(|user| user.id),
			self.wake.clone(),
		));
		self.dirty = true;
	}

	fn open_login(&mut self, window: Arc<winit::window::Window>) {
		let wake = self.wake.clone();
		match platform::LoginView::open(window, move || wake()) {
			Ok(view) => {
				self.login = Some(view);
				self.sign_in_status =
					"Finish signing in on Discord's page. DTEW Cord receives only the session token."
						.into();
			}
			Err(failure) => {
				self.sign_in_status = format!("The login page couldn't open: {failure}");
			}
		}
		self.refresh();
	}

	/// Sends a command to Discord, or answers it locally in the offline preview.
	fn command(&mut self, command: Command) {
		#[cfg(feature = "demo")]
		if self.state.demo {
			self.demo_command(command);
			return;
		}
		let Some(session) = &self.session else {
			return;
		};
		if let Err(rejected) = session.send(command)
			&& let Command::Send { nonce, .. } = *rejected
		{
			self.state.apply(Envelope {
				generation: self.state.generation,
				event: Event::SendResult {
					nonce,
					result: Err(Failure::ProtocolAt("Work queue full; message was not sent")),
				},
			});
		}
	}

	#[cfg(feature = "demo")]
	fn demo_command(&mut self, command: Command) {
		let event = match command {
			Command::History { before, after, .. } => {
				test_support::load_page_with_cursors(&mut self.state, before, after);
				return;
			}
			Command::Send {
				channel,
				content,
				nonce,
				reply,
				..
			} => {
				// A snowflake for "now", so the echo sorts last and reads "Today at …".
				self.demo_ids += 1;
				let id = ((self.clock.now_ms() - 1_420_070_400_000) as u64) << 22 | self.demo_ids;
				let mut message = test_support::message(id, channel);
				message.author = self.state.user.clone().expect("demo user");
				message.author_nick = None;
				message.content = content;
				message.nonce = Some(nonce.clone());
				message.reply_to = reply.map(client_core::Reply::target);
				message.mentions.clear();
				message.mention_everyone = false;
				Event::SendResult {
					nonce,
					result: Ok(message),
				}
			}
			_ => return,
		};
		self.state.apply(Envelope {
			generation: self.state.generation,
			event,
		});
	}

	/// Applies queued session events, login hand-off and session failures, then redraws.
	fn pump(&mut self) {
		if let Some(session) = &self.session {
			let events = session.drain();
			let terminal = session.terminal();
			for envelope in events {
				if envelope.generation == self.state.generation {
					self.state.apply(envelope);
					self.dirty = true;
				}
			}
			if let Some(failure) = terminal {
				self.session_stopped(failure);
			}
		}
		if self.state.auth == AuthState::Authenticated
			&& let Some((secret, save)) = &mut self.secret
			&& *save
		{
			*save = false;
			let secret = secret.clone();
			std::thread::spawn(move || {
				let _ = platform::save_session(&secret);
			});
		}
		if let Some(login) = &self.login {
			login.pump();
			if let Some(secret) = login.token() {
				self.login = None;
				self.connect(secret, true);
			} else if login.expired() {
				let crashed = login.crashed();
				self.login = None;
				self.sign_in_status = if crashed {
					"The login window stopped unexpectedly. No session was accepted.".into()
				} else {
					"Login timed out. No session was accepted.".into()
				};
				self.dirty = true;
			}
		}
		self.refresh();
	}

	fn session_stopped(&mut self, failure: Failure) {
		self.session = None;
		self.state.apply(Envelope {
			generation: self.state.generation,
			event: Event::Failure(failure),
		});
		if !matches!(self.state.auth, AuthState::Expired | AuthState::Challenged) {
			self.state.auth = AuthState::Failed;
		}
		for pending in &mut self.state.pending {
			if pending.delivery == Delivery::Sending {
				pending.delivery = Delivery::Ambiguous;
			}
		}
		if failure == Failure::Expired {
			std::thread::spawn(|| {
				let _ = platform::forget_session();
			});
		}
		self.restoring = false;
		self.sign_in_status = format!("Disconnected: {failure}");
		self.dirty = true;
	}

	fn sign_out(&mut self) {
		self.session = None;
		self.login = None;
		self.secret = None;
		self.state.logout();
		self.place = Place::Home;
		self.collapsed.clear();
		self.opened = None;
		self.restoring = false;
		self.sign_in_status = "Signed out. The saved login was removed.".into();
		std::thread::spawn(|| {
			let _ = platform::forget_session();
		});
		self.dirty = true;
		self.refresh();
	}

	fn select_home(&mut self) {
		self.place = Place::Home;
		self.state.open_home();
		self.dirty = true;
		self.refresh();
	}

	fn select_guild(&mut self, guild: Id) {
		self.place = Place::Guild(guild);
		if let Some(command) = self.state.open_guild(guild) {
			self.command(command);
		}
		self.dirty = true;
		self.refresh();
	}

	fn select_channel(&mut self, channel: Id) {
		if let Some(command) = self.state.select(channel) {
			self.command(command);
		}
		self.dirty = true;
		self.refresh();
	}

	fn send_message(&mut self, text: &str) -> bool {
		let text = text.trim_end();
		if text.trim().is_empty() {
			return false;
		}
		match self.state.prepare_text_send(text) {
			Some(command) => {
				self.command(command);
				self.dirty = true;
				self.refresh();
				true
			}
			None => {
				self.dirty = true;
				self.refresh();
				false
			}
		}
	}

	fn load_older(&mut self) {
		if let Some(command) = self.state.older_history() {
			self.command(command);
			self.dirty = true;
			self.refresh();
		}
	}

	fn screen(&self) -> i32 {
		if self.state.demo {
			return screen::MESSAGING;
		}
		match self.state.auth {
			AuthState::Authenticated if self.state.user.is_some() => screen::MESSAGING,
			// Keep the messaging view on transient drops; the banner says what happened.
			_ if self.state.user.is_some() && self.session.is_some() => screen::MESSAGING,
			AuthState::Authenticating if self.session.is_some() => screen::CONNECTING,
			_ if self.restoring => screen::CONNECTING,
			_ => screen::SIGN_IN,
		}
	}

	/// Rebuilds the window's rows from the state when something changed.
	fn refresh(&mut self) {
		let ui = self.ui();
		ui.set_screen(self.screen());
		ui.set_sign_in_status(self.sign_in_status.as_str().into());
		ui.set_sign_in_busy(self.login.is_some() || self.session.is_some());
		ui.set_restoring(self.restoring);
		ui.set_demo(self.state.demo);
		ui.set_typing_text(view::typing_text(&self.state, Instant::now()).into());
		if !self.dirty {
			return;
		}
		self.dirty = false;

		// The open channel decides the server on screen, unless an empty server was picked.
		if let Some(guild) = self
			.state
			.selected
			.and_then(|id| self.state.channel(id))
			.and_then(|c| c.guild)
		{
			self.place = Place::Guild(guild);
		} else if self.state.selected.is_some() {
			self.place = Place::Home;
		}
		if let Place::Guild(guild) = self.place
			&& self.state.guild(guild).is_none()
		{
			self.place = Place::Home;
		}
		let current = match self.place {
			Place::Guild(guild) => Some(guild),
			Place::Home => None,
		};

		let guilds: Vec<GuildItem> = view::guilds(&self.state, current)
			.into_iter()
			.map(|row| GuildItem {
				id: row.id.0.to_string().into(),
				name: row.name.into(),
				initials: row.initials.into(),
				color: rgb(row.color),
				selected: row.selected,
				unread: row.unread,
			})
			.collect();
		set_rows(&ui.get_guilds(), guilds, |rows| ui.set_guilds(rows));
		ui.set_home_selected(self.place == Place::Home);
		ui.set_home_unread(
			view::direct_channels(&self.state)
				.iter()
				.any(|row| row.unread),
		);

		let (title, channels) = match self.place {
			Place::Home => (
				"Direct Messages".to_owned(),
				view::direct_channels(&self.state),
			),
			Place::Guild(guild) => (
				self.state
					.guild(guild)
					.map_or_else(String::new, |g| g.name.clone()),
				view::guild_channels(&self.state, guild, &self.collapsed),
			),
		};
		ui.set_window_title(
			match &self.state.selected.and_then(|id| self.state.channel(id)) {
				Some(channel) => format!(
					"{} · {title} · DTEW Cord",
					self.state.conversation_name(channel)
				),
				None => format!("{title} · DTEW Cord"),
			}
			.into(),
		);
		ui.set_sidebar_title(title.into());
		let channels: Vec<ChannelItem> = channels
			.into_iter()
			.map(|row| ChannelItem {
				id: row.id.0.to_string().into(),
				name: row.name.into(),
				kind: row.kind,
				selected: row.selected,
				unread: row.unread,
				collapsed: row.collapsed,
				subtitle: row.subtitle.into(),
				initials: row.initials.into(),
				color: rgb(row.color),
			})
			.collect();
		set_rows(&ui.get_channels(), channels, |rows| ui.set_channels(rows));

		let open = self.state.selected.and_then(|id| self.state.channel(id));
		ui.set_has_conversation(open.is_some());
		if let Some(channel) = open {
			let name = self.state.conversation_name(channel).to_owned();
			let kind = view::channel_kind(channel.kind).unwrap_or(view::kind::TEXT);
			ui.set_composer_placeholder(
				match kind {
					view::kind::DIRECT => format!("Message @{name}"),
					view::kind::GROUP => format!("Message {name}"),
					_ => format!("Message #{name}"),
				}
				.into(),
			);
			ui.set_conversation_title(name.into());
			ui.set_conversation_kind(kind);
			ui.set_conversation_subtitle(
				match kind {
					view::kind::GROUP => format!("{} members", channel.recipients.len() + 1),
					_ => String::new(),
				}
				.into(),
			);
			ui.set_can_send(self.state.demo || self.state.auth == AuthState::Authenticated);
		} else {
			ui.set_conversation_title(match self.place {
				Place::Home => "Direct Messages".into(),
				Place::Guild(guild) => self
					.state
					.guild(guild)
					.map_or_else(SharedString::new, |g| g.name.as_str().into()),
			});
			ui.set_conversation_subtitle(SharedString::new());
			ui.set_empty_title("No conversation selected".into());
			ui.set_empty_detail(
				match self.place {
					Place::Home => "Pick a direct message from the list.",
					Place::Guild(_) => "Pick a channel from the list.",
				}
				.into(),
			);
		}
		if self.opened != self.state.selected {
			self.opened = self.state.selected;
			self.serial = self.serial.wrapping_add(1);
			ui.set_conversation_serial(self.serial);
		}

		let messages: Vec<MessageItem> = view::messages(&self.state, &self.clock)
			.into_iter()
			.map(|row| MessageItem {
				id: row.id.into(),
				author: row.author.into(),
				initials: row.initials.into(),
				avatar_color: rgb(row.avatar_color),
				time: row.time.into(),
				short_time: row.short_time.into(),
				markdown: slint::StyledText::from_markdown(&row.markdown)
					.unwrap_or_else(|_| slint::StyledText::from_plain_text(&row.content)),
				content: row.content.into(),
				grouped: row.grouped,
				delivery: row.delivery,
				edited: row.edited,
				divider: row.divider.into(),
				reply: row.reply.into(),
				system: row.system,
				mentions_me: row.mentions_me,
			})
			.collect();
		set_rows(&ui.get_messages(), messages, |rows| ui.set_messages(rows));
		ui.set_loading_older(self.state.history_pending && !self.state.timeline.is_empty());
		ui.set_history_start(self.state.older_exhausted);

		ui.set_banner(if self.state.demo {
			SharedString::new()
		} else if self.state.user.is_some() && self.session.is_none() {
			format!(
				"{} Sign out and in again to reconnect.",
				self.sign_in_status
			)
			.into()
		} else if self.state.user.is_some() && !self.state.gateway_connected {
			"Reconnecting to Discord…".into()
		} else {
			SharedString::new()
		});
		if let Some(user) = &self.state.user {
			let name = self.state.user_display_name(user).to_owned();
			ui.set_user_initials(view::initials(&name).into());
			ui.set_user_color(rgb(view::avatar_color(user.id)));
			ui.set_user_name(name.into());
			ui.set_user_status(
				if self.state.demo {
					"Offline preview".to_owned()
				} else {
					format!("@{}", user.name)
				}
				.into(),
			);
		}
	}
}

/// Replaces a model's rows, editing in place when only a few rows changed so the views keep
/// their scroll position and hover state.
fn set_rows<T: Clone + PartialEq + 'static>(
	model: &ModelRc<T>,
	rows: Vec<T>,
	install: impl FnOnce(ModelRc<T>),
) {
	if let Some(existing) = model.as_any().downcast_ref::<VecModel<T>>() {
		let common = existing.row_count().min(rows.len());
		for (index, row) in rows.iter().enumerate().take(common) {
			if existing.row_data(index).as_ref() != Some(row) {
				existing.set_row_data(index, row.clone());
			}
		}
		if existing.row_count() > rows.len() {
			for index in (rows.len()..existing.row_count()).rev() {
				existing.remove(index);
			}
		}
		for row in rows.into_iter().skip(common) {
			existing.push(row);
		}
	} else {
		install(ModelRc::from(Rc::new(VecModel::from(rows))));
	}
}

/// Opens an http(s) link from a message in the system browser. Anything else is ignored, so
/// message text can't launch local files or other URL handlers.
fn open_link(link: &str) {
	let Ok(url) = url::Url::parse(link) else {
		return;
	};
	if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() {
		return;
	}
	let url = url.to_string();
	#[cfg(target_os = "linux")]
	let opener = std::process::Command::new("xdg-open").arg(&url).spawn();
	#[cfg(target_os = "macos")]
	let opener = std::process::Command::new("open").arg(&url).spawn();
	#[cfg(target_os = "windows")]
	let opener = std::process::Command::new("rundll32")
		.args(["url.dll,FileProtocolHandler", &url])
		.spawn();
	#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
	drop(opener);
}

fn parse_id(text: &SharedString) -> Option<Id> {
	text.parse::<u64>().ok().map(Id)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let demo = std::env::args().any(|arg| arg == "--demo");
	if demo && !cfg!(feature = "demo") {
		eprintln!("The offline preview needs a build with `--features demo`.");
		std::process::exit(2);
	}
	// Read before the runtime starts threads; see `clock`.
	let clock = clock::Clock::local();
	slint::BackendSelector::new()
		.backend_name("winit".into())
		.select()?;
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(2)
		.enable_all()
		.build()?;
	let ui = AppWindow::new()?;
	if let Ok(theme) = std::env::var("DTEWCORD_THEME") {
		ui.global::<Theme>().set_dark(theme != "light");
	}

	APP.with(|slot| {
		*slot.borrow_mut() = Some(App {
			ui: ui.as_weak(),
			runtime,
			clock,
			state: State::default(),
			session: None,
			login: None,
			secret: None,
			place: Place::Home,
			collapsed: BTreeSet::new(),
			opened: None,
			serial: 0,
			sign_in_status: String::new(),
			restoring: false,
			wake: waker(),
			demo_ids: 0,
			dirty: true,
		});
	});

	let weak = ui.as_weak();
	ui.on_sign_in(move || {
		let weak = weak.clone();
		let _ = slint::spawn_local(async move {
			let Some(ui) = weak.upgrade() else { return };
			use slint::winit_030::WinitWindowAccessor;
			match ui.window().winit_window().await {
				Ok(window) => {
					with_app(|app| app.open_login(window));
				}
				Err(_) => {
					with_app(|app| {
						app.sign_in_status = "The login page needs a desktop window.".into();
						app.refresh();
					});
				}
			}
		});
	});
	ui.on_select_home(|| {
		with_app(App::select_home);
	});
	ui.on_select_guild(|id| {
		if let Some(id) = parse_id(&id) {
			with_app(|app| app.select_guild(id));
		}
	});
	ui.on_select_channel(|id| {
		if let Some(id) = parse_id(&id) {
			with_app(|app| app.select_channel(id));
		}
	});
	ui.on_toggle_category(|id| {
		if let Some(id) = parse_id(&id) {
			with_app(|app| {
				if !app.collapsed.remove(&id) {
					app.collapsed.insert(id);
				}
				app.dirty = true;
				app.refresh();
			});
		}
	});
	ui.on_send_message(|text| with_app(|app| app.send_message(&text)).unwrap_or(false));
	ui.on_typed(|_| {});
	ui.on_load_older(|| {
		with_app(App::load_older);
	});
	ui.on_sign_out(|| {
		with_app(App::sign_out);
	});
	let weak = ui.as_weak();
	ui.on_toggle_theme(move || {
		if let Some(ui) = weak.upgrade() {
			let theme = ui.global::<Theme>();
			theme.set_dark(!theme.get_dark());
		}
	});

	ui.on_open_link(|link| open_link(&link));

	// Typing indicators expire and the Linux login window needs its GTK loop pumped.
	let ticker = slint::Timer::default();
	ticker.start(
		slint::TimerMode::Repeated,
		Duration::from_millis(250),
		|| {
			with_app(App::pump);
		},
	);

	with_app(|app| app.begin(demo));
	ui.run()?;
	APP.with(|slot| slot.borrow_mut().take());
	Ok(())
}
