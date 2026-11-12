use crate::Color;
use serde::{Deserialize, Serialize};

/// Every colour a DTEW Cord theme controls. Both clients draw only from these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Token {
	/// Window frame: title bar and server rail.
	Base,
	/// Channel and member lists.
	Sidebar,
	/// Conversation area.
	Chat,
	/// Composer, cards, inputs and popovers.
	Raised,
	Hover,
	Selected,
	Border,
	/// Headings and author names.
	TextStrong,
	Text,
	Muted,
	Link,
	Accent,
	/// Text drawn on top of `accent`.
	AccentText,
	Positive,
	Warning,
	Danger,
	MentionBg,
	MentionText,
}

impl Token {
	pub const COUNT: usize = 18;
	pub const ALL: [Token; Self::COUNT] = [
		Token::Base,
		Token::Sidebar,
		Token::Chat,
		Token::Raised,
		Token::Hover,
		Token::Selected,
		Token::Border,
		Token::TextStrong,
		Token::Text,
		Token::Muted,
		Token::Link,
		Token::Accent,
		Token::AccentText,
		Token::Positive,
		Token::Warning,
		Token::Danger,
		Token::MentionBg,
		Token::MentionText,
	];

	/// The name used in theme files.
	pub fn key(self) -> &'static str {
		match self {
			Token::Base => "base",
			Token::Sidebar => "sidebar",
			Token::Chat => "chat",
			Token::Raised => "raised",
			Token::Hover => "hover",
			Token::Selected => "selected",
			Token::Border => "border",
			Token::TextStrong => "text_strong",
			Token::Text => "text",
			Token::Muted => "muted",
			Token::Link => "link",
			Token::Accent => "accent",
			Token::AccentText => "accent_text",
			Token::Positive => "positive",
			Token::Warning => "warning",
			Token::Danger => "danger",
			Token::MentionBg => "mention_bg",
			Token::MentionText => "mention_text",
		}
	}

	pub fn from_key(key: &str) -> Option<Token> {
		Token::ALL.into_iter().find(|token| token.key() == key)
	}

	/// Surfaces may be translucent; every other token is drawn as given.
	pub fn is_surface(self) -> bool {
		matches!(
			self,
			Token::Base | Token::Sidebar | Token::Chat | Token::Raised
		)
	}

	const fn index(self) -> usize {
		self as usize
	}
}

/// A complete, resolved set of theme colours for one appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
	colors: [Color; Token::COUNT],
	/// Two-stop backdrop gradient (top-left to bottom-right) shown under translucent surfaces.
	pub backdrop: Option<[Color; 2]>,
}

impl Tokens {
	pub fn get(&self, token: Token) -> Color {
		self.colors[token.index()]
	}
	pub fn set(&mut self, token: Token, color: Color) {
		self.colors[token.index()] = color;
	}
	pub fn with(mut self, token: Token, color: Color) -> Self {
		self.set(token, color);
		self
	}
	pub fn iter(&self) -> impl Iterator<Item = (Token, Color)> + '_ {
		Token::ALL.into_iter().map(|token| (token, self.get(token)))
	}

	/// Replace the accent with a user-picked colour and retune what is drawn with it.
	pub fn with_accent(mut self, accent: Color) -> Self {
		let accent = accent.with_alpha(255);
		self.set(Token::Accent, accent);
		self.set(
			Token::AccentText,
			if Color::WHITE.contrast(accent) >= 4.5 {
				Color::WHITE
			} else {
				Color::BLACK
			},
		);
		self.set(Token::MentionBg, accent.with_alpha(76));
		let chat = self.get(Token::Chat);
		let light = accent.mix(Color::WHITE, 0.55);
		let dark = accent.mix(Color::BLACK, 0.45);
		self.set(
			Token::MentionText,
			if light.contrast(chat) >= dark.contrast(chat) {
				light
			} else {
				dark
			},
		);
		self
	}

	pub(crate) const fn from_array(colors: [Color; Token::COUNT]) -> Self {
		Self {
			colors,
			backdrop: None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn token_order_matches_index_and_keys_round_trip() {
		for (index, token) in Token::ALL.into_iter().enumerate() {
			assert_eq!(token.index(), index);
			assert_eq!(Token::from_key(token.key()), Some(token));
		}
		assert_eq!(Token::from_key("nope"), None);
	}
}
