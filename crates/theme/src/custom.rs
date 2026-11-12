use crate::{Builtin, Color, Token, Tokens};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// The theme file format this build reads and writes.
pub const FORMAT: u32 = 1;
/// Longest theme name, in characters.
pub const MAX_NAME: usize = 64;
/// Largest theme file accepted, in bytes. Real files are a few kilobytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// A user's theme: a built-in base plus the colours they changed, per appearance.
///
/// Saved as JSON, for example:
///
/// ```json
/// {
///   "format": 1,
///   "name": "Midnight Mint",
///   "base": "standard",
///   "dark": { "colors": { "accent": "#2fd39a", "chat": "#101418" } },
///   "light": { "colors": { "accent": "#128a63" } }
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomTheme {
	pub format: u32,
	pub name: String,
	/// Key of the built-in theme that supplies every colour not listed here.
	#[serde(default = "default_base")]
	pub base: String,
	#[serde(default, skip_serializing_if = "Overrides::is_empty")]
	pub dark: Overrides,
	#[serde(default, skip_serializing_if = "Overrides::is_empty")]
	pub light: Overrides,
}

fn default_base() -> String {
	Builtin::Standard.key().to_owned()
}

/// Colours a custom theme changes for one appearance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub colors: BTreeMap<Token, Color>,
	/// Replaces the base's backdrop gradient. Only visible where surfaces are translucent.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub backdrop: Option<[Color; 2]>,
}

impl Overrides {
	pub fn is_empty(&self) -> bool {
		self.colors.is_empty() && self.backdrop.is_none()
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeError {
	/// Not valid theme JSON; carries the parser's message.
	Parse(String),
	/// Written by a newer DTEW Cord.
	UnsupportedFormat(u32),
	EmptyName,
	NameTooLong,
	UnknownBase(String),
	TooLarge,
}

impl fmt::Display for ThemeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			ThemeError::Parse(message) => write!(f, "not a DTEW Cord theme: {message}"),
			ThemeError::UnsupportedFormat(format) => write!(
				f,
				"theme format {format} needs a newer DTEW Cord (this build reads format {FORMAT})"
			),
			ThemeError::EmptyName => f.write_str("the theme has no name"),
			ThemeError::NameTooLong => {
				write!(f, "the theme name is longer than {MAX_NAME} characters")
			}
			ThemeError::UnknownBase(base) => write!(f, "unknown base theme {base:?}"),
			ThemeError::TooLarge => write!(f, "the file is larger than {MAX_FILE_BYTES} bytes"),
		}
	}
}

impl std::error::Error for ThemeError {}

impl CustomTheme {
	/// An empty theme on `base`; it looks exactly like the base until colours are changed.
	pub fn new(name: impl Into<String>, base: Builtin) -> Self {
		Self {
			format: FORMAT,
			name: name.into(),
			base: base.key().to_owned(),
			dark: Overrides::default(),
			light: Overrides::default(),
		}
	}

	/// Parse and validate a theme file.
	pub fn from_json(text: &str) -> Result<Self, ThemeError> {
		if text.len() as u64 > MAX_FILE_BYTES {
			return Err(ThemeError::TooLarge);
		}
		let theme: CustomTheme =
			serde_json::from_str(text).map_err(|error| ThemeError::Parse(error.to_string()))?;
		theme.validate()?;
		Ok(theme)
	}

	pub fn to_json(&self) -> String {
		let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
		text.push('\n');
		text
	}

	pub fn validate(&self) -> Result<(), ThemeError> {
		if self.format != FORMAT {
			return Err(ThemeError::UnsupportedFormat(self.format));
		}
		let name = self.name.trim();
		if name.is_empty() {
			return Err(ThemeError::EmptyName);
		}
		if name.chars().count() > MAX_NAME {
			return Err(ThemeError::NameTooLong);
		}
		Builtin::from_key(&self.base).ok_or_else(|| ThemeError::UnknownBase(self.base.clone()))?;
		Ok(())
	}

	/// The built-in theme this one starts from; an unknown key falls back to the house theme.
	pub fn base(&self) -> Builtin {
		Builtin::from_key(&self.base).unwrap_or_default()
	}

	pub fn overrides(&self, dark: bool) -> &Overrides {
		if dark { &self.dark } else { &self.light }
	}

	pub fn overrides_mut(&mut self, dark: bool) -> &mut Overrides {
		if dark {
			&mut self.dark
		} else {
			&mut self.light
		}
	}

	/// Whether the theme picks its own accent for this appearance. Such a theme replaces the
	/// user's accent preference instead of being tinted by it.
	pub fn sets_accent(&self, dark: bool) -> bool {
		self.overrides(dark).colors.contains_key(&Token::Accent)
	}

	/// The theme's full colour set for one appearance, before any accent preference.
	pub fn tokens(&self, dark: bool) -> Tokens {
		let mut tokens = self.base().tokens(dark);
		let overrides = self.overrides(dark);
		for (token, color) in &overrides.colors {
			tokens.set(*token, *color);
		}
		if let Some(backdrop) = overrides.backdrop {
			tokens.backdrop = Some(backdrop);
		}
		tokens
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn round_trips_through_json() {
		let mut theme = CustomTheme::new("Midnight Mint", Builtin::Slate);
		theme
			.dark
			.colors
			.insert(Token::Accent, Color::rgb(0x2fd39a));
		theme.dark.backdrop = Some([Color::rgb(0x102030), Color::rgb(0x000000)]);
		theme
			.light
			.colors
			.insert(Token::MentionBg, Color::rgba(0x2fd39a, 0x40));
		let json = theme.to_json();
		assert!(json.contains("\"accent\": \"#2fd39a\""), "{json}");
		assert!(json.contains("\"mention_bg\": \"#2fd39a40\""), "{json}");
		assert_eq!(CustomTheme::from_json(&json).unwrap(), theme);
	}

	#[test]
	fn minimal_file_uses_the_house_theme() {
		let theme = CustomTheme::from_json(r#"{"format":1,"name":"Plain"}"#).unwrap();
		assert_eq!(theme.base(), Builtin::Standard);
		assert_eq!(theme.tokens(true), Builtin::Standard.tokens(true));
		assert_eq!(theme.tokens(false), Builtin::Standard.tokens(false));
		assert!(!theme.sets_accent(true));
	}

	#[test]
	fn overrides_apply_only_to_their_appearance() {
		let theme = CustomTheme::from_json(
			r##"{"format":1,"name":"Red","dark":{"colors":{"accent":"#ff0000"}}}"##,
		)
		.unwrap();
		assert_eq!(theme.tokens(true).get(Token::Accent), Color::rgb(0xff0000));
		assert_eq!(
			theme.tokens(true).get(Token::Chat),
			Builtin::Standard.tokens(true).get(Token::Chat)
		);
		assert_eq!(
			theme.tokens(false).get(Token::Accent),
			Builtin::Standard.tokens(false).get(Token::Accent)
		);
		assert!(theme.sets_accent(true));
		assert!(!theme.sets_accent(false));
	}

	#[test]
	fn rejects_bad_files() {
		let cases = [
			("{}", "missing"),
			(r#"{"format":2,"name":"x"}"#, "newer"),
			(r#"{"format":1,"name":"  "}"#, "no name"),
			(r#"{"format":1,"name":"x","base":"nope"}"#, "unknown base"),
			(
				r##"{"format":1,"name":"x","dark":{"colors":{"glow":"#fff000"}}}"##,
				"unknown token",
			),
			(
				r#"{"format":1,"name":"x","dark":{"colors":{"chat":"red"}}}"#,
				"bad colour",
			),
			(r#"{"format":1,"name":"x","script":"y"}"#, "unknown field"),
		];
		for (json, why) in cases {
			assert!(CustomTheme::from_json(json).is_err(), "{why}: {json}");
		}
		let long = format!(r#"{{"format":1,"name":"{}"}}"#, "n".repeat(MAX_NAME + 1));
		assert_eq!(CustomTheme::from_json(&long), Err(ThemeError::NameTooLong));
		let huge = " ".repeat(MAX_FILE_BYTES as usize + 1);
		assert_eq!(CustomTheme::from_json(&huge), Err(ThemeError::TooLarge));
	}
}
