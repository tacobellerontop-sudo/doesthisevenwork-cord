//! Maps DTEW Cord's theme tokens onto the `Theme` global in `ui/app.slint`.
use crate::ui::{AppWindow, Theme};
use slint::ComponentHandle;
use theme::{Builtin, Color, CustomTheme, Token, Tokens};

/// What the window shows: a built-in theme or a custom theme file, in one appearance.
pub struct Selection {
	pub custom: Option<CustomTheme>,
	pub builtin: Builtin,
	pub dark: bool,
}

impl Selection {
	/// `DTEWCORD_THEME` picks the theme: `dark` or `light`, a built-in theme's key such as
	/// `slate`, or the path of a theme `.json` file. Anything else keeps the dark house theme.
	pub fn from_env(value: Option<&str>) -> Self {
		let mut selection = Self {
			custom: None,
			builtin: Builtin::Standard,
			dark: true,
		};
		match value.map(str::trim) {
			None | Some("" | "dark") => {}
			Some("light") => selection.dark = false,
			Some(value) => {
				if let Some(builtin) = Builtin::from_key(value) {
					selection.builtin = builtin;
				} else {
					match std::fs::read_to_string(value)
						.map_err(|error| error.to_string())
						.and_then(|text| {
							CustomTheme::from_json(&text).map_err(|error| error.to_string())
						}) {
						Ok(custom) => selection.custom = Some(custom),
						Err(error) => eprintln!("DTEWCORD_THEME: {value}: {error}"),
					}
				}
			}
		}
		selection
	}

	pub fn tokens(&self) -> Tokens {
		match &self.custom {
			Some(custom) => custom.tokens(self.dark),
			None => self.builtin.tokens(self.dark),
		}
	}

	pub fn apply(&self, ui: &AppWindow) {
		let global = ui.global::<Theme>();
		global.set_dark(self.dark);
		let colors = Colors::from_tokens(&self.tokens());
		global.set_rail(colors.rail);
		global.set_sidebar(colors.sidebar);
		global.set_chat(colors.chat);
		global.set_raised(colors.raised);
		global.set_panel(colors.panel);
		global.set_hover(colors.hover);
		global.set_selected(colors.selected);
		global.set_input(colors.input);
		global.set_border(colors.border);
		global.set_text(colors.text);
		global.set_strong(colors.strong);
		global.set_muted(colors.muted);
		global.set_faint(colors.faint);
		global.set_link(colors.link);
		global.set_accent(colors.accent);
		global.set_accent_hover(colors.accent_hover);
		global.set_accent_text(colors.accent_text);
		global.set_danger(colors.danger);
		global.set_success(colors.success);
		global.set_warning(colors.warning);
		global.set_warning_surface(colors.warning_surface);
		global.set_mention(colors.mention);
		global.set_mention_bar(colors.mention_bar);
		global.set_message_hover(colors.message_hover);
	}
}

/// Every colour the Slint views use, derived from the theme tokens.
struct Colors {
	rail: slint::Color,
	sidebar: slint::Color,
	chat: slint::Color,
	raised: slint::Color,
	panel: slint::Color,
	hover: slint::Color,
	selected: slint::Color,
	input: slint::Color,
	border: slint::Color,
	text: slint::Color,
	strong: slint::Color,
	muted: slint::Color,
	faint: slint::Color,
	link: slint::Color,
	accent: slint::Color,
	accent_hover: slint::Color,
	accent_text: slint::Color,
	danger: slint::Color,
	success: slint::Color,
	warning: slint::Color,
	warning_surface: slint::Color,
	mention: slint::Color,
	mention_bar: slint::Color,
	message_hover: slint::Color,
}

impl Colors {
	fn from_tokens(tokens: &Tokens) -> Self {
		// Gradient themes have translucent surfaces; this client paints no backdrop, so they
		// are composited onto the gradient's midpoint.
		let ground = tokens
			.backdrop
			.map(|[top, bottom]| top.mix(bottom, 0.5).with_alpha(255));
		let solid = |token: Token| flatten(tokens.get(token), ground);
		let chat = solid(Token::Chat);
		let base = solid(Token::Base);
		let sidebar = solid(Token::Sidebar);
		let accent = tokens.get(Token::Accent);
		let warning = tokens.get(Token::Warning);
		Self {
			rail: color(base),
			sidebar: color(sidebar),
			chat: color(chat),
			raised: color(solid(Token::Raised)),
			panel: color(sidebar.mix(base, 0.5)),
			hover: color(flatten(tokens.get(Token::Hover), Some(sidebar))),
			selected: color(flatten(tokens.get(Token::Selected), Some(sidebar))),
			input: color(solid(Token::Raised)),
			border: color(flatten(tokens.get(Token::Border), Some(chat))),
			text: color(tokens.get(Token::Text)),
			strong: color(tokens.get(Token::TextStrong)),
			muted: color(tokens.get(Token::Muted)),
			faint: color(tokens.get(Token::Muted).mix(chat, 0.3)),
			link: color(tokens.get(Token::Link)),
			accent: color(accent),
			accent_hover: color(accent.mix(Color::BLACK, 0.18)),
			accent_text: color(tokens.get(Token::AccentText)),
			danger: color(tokens.get(Token::Danger)),
			success: color(tokens.get(Token::Positive)),
			warning: color(warning),
			warning_surface: color(chat.mix(warning.with_alpha(255), 0.16)),
			mention: color(tokens.get(Token::MentionBg)),
			mention_bar: color(accent),
			message_hover: color(flatten(tokens.get(Token::Hover), Some(chat)).mix(chat, 0.5)),
		}
	}
}

/// `color` composited over an opaque `ground`; without one, the colour is made opaque as is.
fn flatten(color: Color, ground: Option<Color>) -> Color {
	match ground {
		Some(ground) if !color.is_opaque() => {
			ground.mix(color.with_alpha(255), f32::from(color.a) / 255.0)
		}
		_ => color.with_alpha(255),
	}
}

fn color(color: Color) -> slint::Color {
	slint::Color::from_argb_u8(color.a, color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn selection_reads_mode_keys_and_files() {
		let dark = Selection::from_env(None);
		assert!(dark.dark && dark.custom.is_none() && dark.builtin == Builtin::Standard);
		assert!(!Selection::from_env(Some("light")).dark);
		assert_eq!(Selection::from_env(Some("slate")).builtin, Builtin::Slate);
		assert_eq!(
			Selection::from_env(Some("nope.json")).builtin,
			Builtin::Standard
		);

		let file =
			std::env::temp_dir().join(format!("dtewcord-slint-theme-{}.json", std::process::id()));
		let mut custom = CustomTheme::new("Mint", Builtin::Eclipse);
		custom
			.dark
			.colors
			.insert(Token::Accent, Color::rgb(0x2fd39a));
		std::fs::write(&file, custom.to_json()).unwrap();
		let selection = Selection::from_env(file.to_str());
		std::fs::remove_file(&file).unwrap();
		assert_eq!(selection.tokens().get(Token::Accent), Color::rgb(0x2fd39a));
	}

	#[test]
	fn gradient_themes_become_opaque() {
		for builtin in Builtin::ALL {
			let colors = Colors::from_tokens(&builtin.tokens(true));
			for surface in [
				colors.rail,
				colors.sidebar,
				colors.chat,
				colors.raised,
				colors.hover,
			] {
				assert_eq!(surface.alpha(), 255, "{builtin:?}");
			}
		}
	}
}
