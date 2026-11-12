use crate::{Color, Token, Tokens};

/// DTEW Cord azure, the house accent.
pub const DEFAULT_ACCENT: Color = Color::rgb(0x1a72e8);
/// Dark themes' danger red, tuned for red text and outlines on dark surfaces.
pub const DARK_DANGER: Color = Color::rgb(0xef5561);

/// The themes that ship with DTEW Cord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Builtin {
	#[default]
	Standard,
	Eclipse,
	Slate,
	Nightfall,
	Ember,
	Verdant,
	Afterglow,
}

impl Builtin {
	pub const ALL: [Builtin; 7] = [
		Builtin::Standard,
		Builtin::Eclipse,
		Builtin::Slate,
		Builtin::Nightfall,
		Builtin::Ember,
		Builtin::Verdant,
		Builtin::Afterglow,
	];

	pub fn label(self) -> &'static str {
		match self {
			Builtin::Standard => "DTEW Cord",
			Builtin::Eclipse => "Eclipse",
			Builtin::Slate => "Slate",
			Builtin::Nightfall => "Nightfall",
			Builtin::Ember => "Ember",
			Builtin::Verdant => "Verdant",
			Builtin::Afterglow => "Afterglow",
		}
	}

	/// Stable identifier for settings and for the `base` of custom theme files.
	pub fn key(self) -> &'static str {
		match self {
			Builtin::Standard => "standard",
			Builtin::Eclipse => "eclipse",
			Builtin::Slate => "slate",
			Builtin::Nightfall => "nightfall",
			Builtin::Ember => "ember",
			Builtin::Verdant => "verdant",
			Builtin::Afterglow => "afterglow",
		}
	}

	/// Keys written by builds that shipped the previous theme names.
	fn legacy_key(self) -> &'static str {
		match self {
			Builtin::Standard => "standard",
			Builtin::Eclipse => "onyx",
			Builtin::Slate => "ash",
			Builtin::Nightfall => "midnight-blurple",
			Builtin::Ember => "crimson-moon",
			Builtin::Verdant => "forest",
			Builtin::Afterglow => "sunset",
		}
	}

	pub fn from_key(key: &str) -> Option<Builtin> {
		Builtin::ALL
			.into_iter()
			.find(|theme| theme.key() == key || theme.legacy_key() == key)
	}

	pub fn from_u8(value: u8) -> Builtin {
		Builtin::ALL
			.into_iter()
			.find(|theme| *theme as u8 == value)
			.unwrap_or_default()
	}

	/// Gradient themes ignore the light/dark preference and always use light text.
	pub fn is_gradient(self) -> bool {
		matches!(
			self,
			Builtin::Nightfall | Builtin::Ember | Builtin::Verdant | Builtin::Afterglow
		)
	}

	/// Only the house theme has a light version; the others are dark in both appearances.
	pub fn has_light(self) -> bool {
		self == Builtin::Standard
	}

	/// The theme's colours with the house accent.
	pub fn tokens(self, dark: bool) -> Tokens {
		match self {
			Builtin::Standard if dark => dark_common(
				[0x0e0e10, 0x141416, 0x18181b, 0x202023],
				[0x242427, 0x2e2e32, 0x27272a],
			),
			Builtin::Standard => light(),
			Builtin::Eclipse => dark_common(
				[0x000000, 0x070708, 0x070708, 0x141416],
				[0x17171a, 0x222226, 0x1c1c20],
			),
			Builtin::Slate => dark_common(
				[0x1b1f2a, 0x262b38, 0x2c3140, 0x343a4b],
				[0x313747, 0x3d4456, 0x3a4152],
			),
			Builtin::Nightfall => gradient([0x35519f, 0x1b1440], 0x2f3451),
			Builtin::Ember => gradient([0x8c2340, 0x140609], 0x41262e),
			Builtin::Verdant => gradient([0x2b6350, 0x081a15], 0x283934),
			Builtin::Afterglow => gradient([0xd9663c, 0x35205e], 0x5b3b40),
		}
	}
}

/// `surfaces` are base, sidebar, chat and raised; `states` are hover, selected and border.
fn dark_common(surfaces: [u32; 4], states: [u32; 3]) -> Tokens {
	let [base, sidebar, chat, raised] = surfaces.map(Color::rgb);
	let [hover, selected, border] = states.map(Color::rgb);
	dark_with(base, sidebar, chat, raised, hover, selected, border)
}

fn dark_with(
	base: Color,
	sidebar: Color,
	chat: Color,
	raised: Color,
	hover: Color,
	selected: Color,
	border: Color,
) -> Tokens {
	Tokens::from_array([
		base,
		sidebar,
		chat,
		raised,
		hover,
		selected,
		border,
		Color::rgb(0xededef),
		Color::rgb(0xcfcfd4),
		Color::rgb(0x94949c),
		Color::rgb(0x54abff),
		DEFAULT_ACCENT,
		Color::WHITE,
		Color::rgb(0x2fb87a),
		Color::rgb(0xe8a33d),
		DARK_DANGER,
		DEFAULT_ACCENT.with_alpha(76),
		Color::rgb(0xbcd9ff),
	])
}

fn light() -> Tokens {
	Tokens::from_array([
		Color::rgb(0xe4e4e7),
		Color::rgb(0xf2f2f4),
		Color::WHITE,
		Color::rgb(0xebebee),
		Color::rgb(0xe8e8eb),
		Color::rgb(0xdadade),
		Color::rgb(0xe0e0e4),
		Color::rgb(0x0f0f11),
		Color::rgb(0x2f2f34),
		Color::rgb(0x62626a),
		Color::rgb(0x0b63d6),
		DEFAULT_ACCENT,
		Color::WHITE,
		Color::rgb(0x1c9c63),
		Color::rgb(0xc8860f),
		Color::rgb(0xd23742),
		DEFAULT_ACCENT.with_alpha(76),
		Color::rgb(0x14508f),
	])
}

/// Translucent surfaces over a two-stop backdrop, with an opaque border for crisp edges at
/// fractional scales.
fn gradient(stops: [u32; 2], border: u32) -> Tokens {
	let mut tokens = dark_with(
		Color::rgba(0x000000, 140),
		Color::rgba(0x000000, 90),
		Color::rgba(0x000000, 90),
		Color::rgba(0x000000, 120),
		Color::rgba(0xffffff, 18),
		Color::rgba(0xffffff, 34),
		Color::rgb(border),
	);
	tokens.set(Token::Text, Color::rgb(0xe6eaf2));
	tokens.set(Token::Muted, Color::rgb(0xafb6c4));
	tokens.backdrop = Some(stops.map(Color::rgb));
	tokens
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn keys_round_trip_including_legacy_names() {
		for theme in Builtin::ALL {
			assert_eq!(Builtin::from_key(theme.key()), Some(theme));
			assert_eq!(Builtin::from_u8(theme as u8), theme);
		}
		assert_eq!(Builtin::from_key("onyx"), Some(Builtin::Eclipse));
		assert_eq!(Builtin::from_key("sunset"), Some(Builtin::Afterglow));
		assert_eq!(Builtin::from_key("custom"), None);
	}

	#[test]
	fn text_is_readable_on_every_builtin() {
		for theme in Builtin::ALL {
			for dark in [true, false] {
				let tokens = theme.tokens(dark);
				let chat = tokens
					.backdrop
					.map_or(tokens.get(Token::Chat), |[top, bottom]| {
						top.mix(bottom, 0.5)
					});
				assert!(
					tokens.get(Token::Text).contrast(chat) >= 4.5,
					"{theme:?} dark={dark}"
				);
				assert!(
					tokens
						.get(Token::AccentText)
						.contrast(tokens.get(Token::Accent))
						>= 4.5,
					"{theme:?} dark={dark}"
				);
			}
		}
	}

	#[test]
	fn only_gradients_have_backdrops() {
		for theme in Builtin::ALL {
			assert_eq!(theme.tokens(true).backdrop.is_some(), theme.is_gradient());
		}
	}
}
