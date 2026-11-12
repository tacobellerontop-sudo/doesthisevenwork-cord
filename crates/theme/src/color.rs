use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// An sRGB colour with straight (unmultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
	pub r: u8,
	pub g: u8,
	pub b: u8,
	pub a: u8,
}

impl Color {
	pub const WHITE: Color = Color::rgb(0xffffff);
	pub const BLACK: Color = Color::rgb(0x000000);

	/// `0xRRGGBB`, fully opaque.
	pub const fn rgb(value: u32) -> Self {
		Self {
			r: (value >> 16) as u8,
			g: (value >> 8) as u8,
			b: value as u8,
			a: 255,
		}
	}
	/// `0xRRGGBB` with a separate alpha.
	pub const fn rgba(value: u32, a: u8) -> Self {
		let Color { r, g, b, .. } = Self::rgb(value);
		Self { r, g, b, a }
	}
	pub const fn with_alpha(self, a: u8) -> Self {
		Self { a, ..self }
	}
	pub const fn to_array(self) -> [u8; 4] {
		[self.r, self.g, self.b, self.a]
	}
	pub const fn is_opaque(self) -> bool {
		self.a == 255
	}

	/// `#RRGGBB` or `#RRGGBBAA`; the leading `#` is optional. Surrounding space is ignored.
	pub fn parse(text: &str) -> Result<Self, ColorError> {
		let text = text.trim();
		let hex = text.strip_prefix('#').unwrap_or(text);
		if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
			return Err(ColorError(text.to_owned()));
		}
		let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or_default();
		Ok(Self {
			r: byte(0),
			g: byte(2),
			b: byte(4),
			a: if hex.len() == 8 { byte(6) } else { 255 },
		})
	}

	/// Linear interpolation in sRGB space, `t = 0` is `self`.
	pub fn mix(self, other: Color, t: f32) -> Color {
		let t = t.clamp(0.0, 1.0);
		let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
		Color {
			r: lerp(self.r, other.r),
			g: lerp(self.g, other.g),
			b: lerp(self.b, other.b),
			a: lerp(self.a, other.a),
		}
	}

	/// WCAG relative luminance of the colour's RGB, ignoring alpha.
	pub fn luminance(self) -> f32 {
		let channel = |value: u8| {
			let value = f32::from(value) / 255.0;
			if value <= 0.040_45 {
				value / 12.92
			} else {
				((value + 0.055) / 1.055).powf(2.4)
			}
		};
		0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
	}

	/// WCAG contrast ratio between two colours, from 1 to 21.
	pub fn contrast(self, other: Color) -> f32 {
		let (a, b) = (self.luminance() + 0.05, other.luminance() + 0.05);
		a.max(b) / a.min(b)
	}
}

impl fmt::Display for Color {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
		if self.a != 255 {
			write!(f, "{:02x}", self.a)?;
		}
		Ok(())
	}
}

impl Serialize for Color {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.collect_str(self)
	}
}

impl<'de> Deserialize<'de> for Color {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
		Color::parse(&text).map_err(serde::de::Error::custom)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorError(pub String);

impl fmt::Display for ColorError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?} is not a #RRGGBB or #RRGGBBAA colour", self.0)
	}
}

impl std::error::Error for ColorError {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_and_prints_hex() {
		assert_eq!(Color::parse("#1a72e8").unwrap(), Color::rgb(0x1a72e8));
		assert_eq!(Color::parse(" 1A72E8 ").unwrap(), Color::rgb(0x1a72e8));
		assert_eq!(
			Color::parse("#00000080").unwrap(),
			Color::rgba(0x000000, 0x80)
		);
		assert_eq!(Color::rgb(0x1a72e8).to_string(), "#1a72e8");
		assert_eq!(Color::rgba(0xffffff, 18).to_string(), "#ffffff12");
		for bad in [
			"",
			"#",
			"#fff",
			"#12345",
			"#1234567",
			"#gggggg",
			"#1a72e8ff00",
		] {
			assert!(Color::parse(bad).is_err(), "{bad}");
		}
	}

	#[test]
	fn contrast_matches_wcag() {
		assert!((Color::WHITE.contrast(Color::BLACK) - 21.0).abs() < 0.01);
		assert!((Color::WHITE.contrast(Color::WHITE) - 1.0).abs() < 0.001);
	}
}
