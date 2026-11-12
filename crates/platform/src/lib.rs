//! Credential persistence and temporary owner-operated login/verification surfaces.
pub mod audio;
pub mod badge;
pub mod captcha;
pub mod compositor;
pub mod game_activity;
pub mod heic;
pub mod hotkeys;
pub mod idle;
mod login_keyring;
pub mod memory;
pub mod notifications;
pub mod plaintext_login;
pub mod pointer;
pub mod processes;
pub mod save;
pub mod single_instance;
pub mod startup;
pub mod tray;
pub mod video;
pub mod window_effects;
use client_core::auth::{Failure, SessionSecret};
pub use pointer::cursor_position;
use std::{
	sync::{
		Arc,
		mpsc::{self, Receiver},
	},
	time::{Duration, Instant},
};
use wry::{WebView, WebViewBuilder};

/// Logical height of the native header the desktop app draws above the login webview.
pub const LOGIN_HEADER_HEIGHT: f32 = 56.0;
const SERVICE: &str = "io.github.tacobellerontop_sudo.dtewcord";
#[cfg(not(feature = "development-data"))]
const ACCOUNT: &str = "discord-session";
#[cfg(feature = "development-data")]
const ACCOUNT: &str = "discord-session.development";

/// Whether this process may start another one at all. A security product that refuses the shell
/// for an unknown binary usually refuses process creation too, and the difference is what a report
/// needs; `Err` carries the operating system's code. The probe writes nothing and opens no window.
pub fn spawn_probe() -> Result<(), i64> {
	{
		const CREATE_NO_WINDOW: u32 = 0x0800_0000;
		use std::os::windows::process::CommandExt;
		std::process::Command::new("cmd.exe")
			.args(["/c", "exit"])
			.creation_flags(CREATE_NO_WINDOW)
			.status()
			.map(drop)
			.map_err(|error| i64::from(error.raw_os_error().unwrap_or(-1)))
	}
}

/// Opens a folder in the owner's file manager. The caller passes a path it already owns, so this
/// crosses no service or privilege boundary. `Err` carries the platform's failure code, which the
/// caller records instead of leaving a button that appears to do nothing.
pub fn open_folder(path: &std::path::Path) -> Result<(), i64> {
	open_with_shell(path)
}

/// Windows cannot simply start `explorer.exe` (a fresh one fails to initialize its DLLs,
/// 0xc0000142) and a machine policy can refuse this process the shell entirely
/// (SE_ERR_ACCESSDENIED), so two quiet routes are tried before the caller hands the path over.
/// `ShellExecuteW` is deliberately absent: on a machine that refuses it, it puts a system dialog
/// on screen instead of failing quietly.
fn open_with_shell(path: &std::path::Path) -> Result<(), i64> {
	// The running shell browses the folder through COM; this never starts explorer.exe.
	let selected = select_in_shell(path);
	if selected.is_ok() {
		return Ok(());
	}
	// A trusted command interpreter asks the shell on our behalf.
	if start_via_cmd(path).is_ok() {
		return Ok(());
	}
	// The first code names the policy that refused us, so it is the one worth reporting.
	selected
}

/// Initializes an apartment for the shell calls. A render thread usually has none and the shell
/// then reports SE_ERR_ACCESSDENIED. An existing different apartment is fine: the thread keeps
/// whichever apartment it already had, so nothing is uninitialized here.
#[allow(unsafe_code)]
fn prepare_shell() {
	use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
	// SAFETY: no memory or handles are passed; the call only sets the thread's apartment.
	unsafe {
		let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
	}
}

/// A NUL-terminated absolute path the shell calls can read, or `None` for an unrepresentable one.
fn wide_path(path: &std::path::Path) -> Option<Vec<u16>> {
	use std::os::windows::ffi::OsStrExt;
	let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
	let mut wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
	if wide.contains(&0) {
		return None;
	}
	wide.push(0);
	Some(wide)
}

/// Asks the running shell to browse the folder: no new process, and the shell that already owns
/// the desktop is the one that answers.
#[allow(unsafe_code)]
fn select_in_shell(path: &std::path::Path) -> Result<(), i64> {
	use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
	use windows::core::PCWSTR;

	prepare_shell();
	let wide = wide_path(path).ok_or(-1)?;
	// SAFETY: `wide` is NUL-terminated and outlives both calls; the item list comes from the
	// shell's own allocator and is released with ILFree, and no extra items are passed.
	unsafe {
		let list = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
		if list.is_null() {
			return Err(-1);
		}
		let result = SHOpenFolderAndSelectItems(list, None, 0);
		ILFree(Some(list));
		result.map_err(|error| i64::from(error.code().0))
	}
}

/// Last resort: `start` runs inside a trusted command interpreter, which the shell is more likely
/// to accept than this process.
fn start_via_cmd(path: &std::path::Path) -> Result<(), i64> {
	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	use std::os::windows::process::CommandExt;
	let status = std::process::Command::new("cmd.exe")
		.args(["/c", "start", "", &path.display().to_string()])
		.creation_flags(CREATE_NO_WINDOW)
		.status();
	match status {
		Ok(status) if status.success() => Ok(()),
		Ok(status) => Err(i64::from(status.code().unwrap_or(-1))),
		Err(error) => Err(i64::from(error.raw_os_error().unwrap_or(-1))),
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialError {
	/// No OS credential store exists (e.g. Linux without a Secret Service provider).
	NoStore,
	Unavailable,
	Invalid,
	TimedOut,
}

/// Where a saved login ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
	CredentialStore,
	/// The owner opted into the plaintext fallback and no credential store accepted it.
	Plaintext,
}
/// The entry restored on launch. Switching accounts rewrites it from the per-account entry.
pub fn load_session() -> Result<Option<SessionSecret>, CredentialError> {
	load_login(ACCOUNT)
}
pub fn save_session(secret: &SessionSecret) -> Result<Saved, CredentialError> {
	save_login(ACCOUNT, secret)
}
pub fn forget_session() -> Result<(), CredentialError> {
	forget_login(ACCOUNT)
}
/// One entry per remembered account, so the switcher never keeps a second copy in memory.
fn account_entry(account: model::Id) -> String {
	format!("{ACCOUNT}.{account}")
}
pub fn load_account_session(account: model::Id) -> Result<Option<SessionSecret>, CredentialError> {
	load_login(&account_entry(account))
}
pub fn save_account_session(
	account: model::Id,
	secret: &SessionSecret,
) -> Result<Saved, CredentialError> {
	save_login(&account_entry(account), secret)
}
pub fn forget_account_session(account: model::Id) -> Result<(), CredentialError> {
	forget_login(&account_entry(account))
}
/// A fallback file is authoritative until a confirmed OS save removes it. A marker
/// remains after deletion to prevent an older (or late) keyring write restoring it.
fn load_login(name: &str) -> Result<Option<SessionSecret>, CredentialError> {
	let (directory, enabled) = plaintext_login::configuration();
	if let Some(directory) = directory {
		if enabled && let Some(secret) = plaintext_login::load(&directory, name)? {
			return Ok(Some(secret));
		}
		if plaintext_login::keyring_stale(&directory, name)? {
			return Ok(None);
		}
	}
	login_keyring::load(name)
}
fn save_login(name: &str, secret: &SessionSecret) -> Result<Saved, CredentialError> {
	let (directory, enabled) = plaintext_login::configuration();
	// Refresh any existing fallback before native IO: a cleanup error must never leave
	// an older plaintext token authoritative after a successful keyring write.
	if enabled
		&& let Some(directory) = directory.as_ref()
		&& plaintext_login::keyring_stale(directory, name)?
	{
		plaintext_login::save(directory, name, secret)?;
	}
	match login_keyring::save(name, secret) {
		Ok(()) => {
			if let Some(directory) = directory {
				plaintext_login::keyring_current(&directory, name)?;
			}
			Ok(Saved::CredentialStore)
		}
		Err(error) => {
			if enabled && let Some(directory) = directory {
				plaintext_login::save(&directory, name, secret)?;
				Ok(Saved::Plaintext)
			} else {
				Err(error)
			}
		}
	}
}
fn forget_login(name: &str) -> Result<(), CredentialError> {
	let (directory, _) = plaintext_login::configuration();
	// Fence even a late default-backend save before attempting native deletion. An
	// unavailable/full native queue must never resurrect a logged-out launch entry.
	let local = directory.as_ref().map_or(Ok(()), |directory| {
		plaintext_login::mark_stale(directory, name)?;
		plaintext_login::forget(directory, name)
	});
	let stored = login_keyring::forget(name);
	local?;
	// Forgotten roster slots can be released when no OS store exists. The launch
	// marker stays until confirmed deletion, preventing wrong-account restoration.
	if (stored.is_ok() || (stored == Err(CredentialError::NoStore) && name != ACCOUNT))
		&& let Some(directory) = directory
	{
		plaintext_login::keyring_current(&directory, name)?;
	}
	match stored {
		Err(CredentialError::NoStore) => Ok(()),
		stored => stored,
	}
}

fn entry(name: &str) -> Result<keyring_core::Entry, CredentialError> {
	credential_store()?;
	keyring_core::Entry::new(SERVICE, name).map_err(|error| match error {
		keyring_core::Error::NoDefaultStore => CredentialError::NoStore,
		_ => CredentialError::Unavailable,
	})
}
fn credential_store() -> Result<(), CredentialError> {
	keyring::Entry::store_status()
		.as_ref()
		.copied()
		.map_err(|_| CredentialError::NoStore)
}
fn load_entry(name: &str) -> Result<Option<SessionSecret>, CredentialError> {
	match entry(name)?.get_password() {
		Ok(value) => SessionSecret::from_owner_input(value)
			.map(Some)
			.map_err(|_| CredentialError::Invalid),
		Err(keyring_core::Error::NoEntry) => Ok(None),
		Err(_) => Err(CredentialError::Unavailable),
	}
}
fn save_entry(name: &str, secret: &SessionSecret) -> Result<(), CredentialError> {
	entry(name)?
		.set_password(secret.expose())
		.map_err(|_| CredentialError::Unavailable)
}
fn forget_entry(name: &str) -> Result<(), CredentialError> {
	match entry(name)?.delete_credential() {
		Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
		Err(_) => Err(CredentialError::Unavailable),
	}
}
fn discord_origin(value: &str) -> bool {
	url::Url::parse(value).is_ok_and(|url| {
		url.scheme() == "https"
			&& url.host_str() == Some("discord.com")
			&& url.port_or_known_default() == Some(443)
			&& url.username().is_empty()
			&& url.password().is_none()
	})
}
fn login_navigation(value: &str) -> bool {
	discord_origin(value) || captcha::hcaptcha_origin(value)
}
/// Receives only the account token used by THIS ephemeral, owner-operated login page.
/// No browser-profile reads, password interception, console instructions, or QR exchange implementation.
pub struct LoginView {
	view: WebView,
	tokens: Receiver<SessionSecret>,
	opened: Instant,
}
impl LoginView {
	pub fn open(
		parent: Arc<winit::window::Window>,
		wake: impl Fn() + Send + Sync + 'static,
	) -> Result<Self, Failure> {
		let (send, tokens) = mpsc::sync_channel(1);
		let mut random = [0_u8; 32];
		getrandom::fill(&mut random).map_err(|_| Failure::Protocol)?;
		let capability = random
			.iter()
			.map(|byte| format!("{byte:02x}"))
			.collect::<String>()
			+ ":";
		let script =
			include_str!("login-handoff.js").replace("__DTEWCORD_LOGIN_CAPABILITY__", &capability);
		let builder = WebViewBuilder::new()
			.with_url("https://discord.com/login")
			.with_incognito(true)
			.with_devtools(false)
			.with_initialization_script_for_main_only(script, true)
			.with_navigation_handler(|url| login_navigation(&url))
			.with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
			.with_download_started_handler(|_, _| false)
			.with_ipc_handler(move |request| {
				if !discord_origin(&request.uri().to_string()) || request.body().len() > 2113 {
					return;
				}
				let body = zeroize::Zeroizing::new(request.into_body());
				let Some(value) = body.strip_prefix(&capability) else {
					return;
				};
				if let Ok(secret) = SessionSecret::from_owner_input(value.to_owned()) {
					let _ = send.try_send(secret);
					wake();
				}
			});
		let view = builder
			.with_bounds(bounds(&parent))
			.build_as_child(parent.as_ref())
			.map_err(|_| Failure::Protocol)?;
		Ok(Self {
			view,
			tokens,
			opened: Instant::now(),
		})
	}
	pub fn token(&self) -> Option<SessionSecret> {
		self.tokens.try_recv().ok()
	}
	pub fn expired(&self) -> bool {
		self.opened.elapsed() > Duration::from_secs(600)
	}
	pub fn crashed(&self) -> bool {
		false
	}
	pub fn resize(&self, parent: &winit::window::Window) {
		let _ = self.view.set_bounds(bounds(parent));
	}
	pub fn pump(&self) {}
}
fn bounds(parent: &winit::window::Window) -> wry::Rect {
	let size = parent.inner_size();
	let header = (LOGIN_HEADER_HEIGHT as f64 * parent.scale_factor()).round() as u32;
	wry::Rect {
		position: wry::dpi::PhysicalPosition::new(0, header as i32).into(),
		size: wry::dpi::PhysicalSize::new(size.width, size.height.saturating_sub(header)).into(),
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn credential_account_names_are_profile_scoped() {
		#[cfg(feature = "development-data")]
		assert_eq!(ACCOUNT, "discord-session.development");
		#[cfg(not(feature = "development-data"))]
		assert_eq!(ACCOUNT, "discord-session");
		assert_eq!(account_entry(model::Id(7)), format!("{ACCOUNT}.7"));
	}
	#[test]
	fn handoff_accepts_only_our_discord_origin() {
		assert!(discord_origin("https://discord.com/login"));
		for value in [
			"http://discord.com",
			"https://discord.com.evil.test",
			"https://evil.test/discord.com",
			"https://user@discord.com",
			"https://discord.com:444",
		] {
			assert!(!discord_origin(value));
		}
		let script = include_str!("login-handoff.js");
		assert!(!script.contains("localStorage"));
		assert!(!script.contains("password"));

		{
			assert!(login_navigation("https://newassets.hcaptcha.com/captcha/"));
			assert!(!login_navigation("http://hcaptcha.com/"));
			assert!(!login_navigation("https://hcaptcha.com.evil.test/"));
		}
	}
}
