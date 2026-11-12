//! Theme palette and style types, kept only until the standalone theming system replaces them.
//! The extension package system (plugins, catalog and Wasm runtime) was removed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Theme exceeds its resource limit")]
	Limit,
	#[error("Invalid theme document")]
	Invalid,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePalette {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub background: Option<Background>,
	#[serde(default)]
	pub colors: BTreeMap<String, String>,
	#[serde(default)]
	pub backdrop: Option<[String; 2]>,
}

/// Image settings for the package's single embedded static background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Background {
	pub opacity: u8,
	pub fit: BackgroundFit,
	pub target: BackgroundTarget,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub sections: Option<SectionOpacity>,
}

impl Default for Background {
	fn default() -> Self {
		Self {
			opacity: 25,
			fit: BackgroundFit::Cover,
			target: BackgroundTarget::Window,
			sections: None,
		}
	}
}

/// Surface coverage over one continuous window image; text and controls stay opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SectionOpacity {
	pub top_bar: u8,
	pub server_list: u8,
	pub channel_list: u8,
	pub message_list: u8,
	pub member_list: u8,
	pub composer: u8,
}
impl Default for SectionOpacity {
	fn default() -> Self {
		Self {
			top_bar: 85,
			server_list: 85,
			channel_list: 85,
			message_list: 75,
			member_list: 85,
			composer: 90,
		}
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundFit {
	#[default]
	Cover,
	Contain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTarget {
	#[default]
	Window,
	Chat,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
	#[serde(default)]
	pub light: ThemePalette,
	#[serde(default)]
	pub dark: ThemePalette,
	#[serde(default)]
	pub style: ThemeStyle,
}

/// Native control metrics in logical pixels. Omitted fields inherit the active theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeStyle {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transparency_blur: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transparency: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blur: Option<u8>,
	/// Legacy switch: transparency now always covers every surface. Older themes that still
	/// set it keep parsing; the value is ignored and never written back.
	#[serde(skip_serializing)]
	pub transparent_all: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub body_size: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub heading_size: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub button_size: Option<u8>,
	/// Shared icon glyph size baseline in logical pixels; hit regions stay independent.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub icon_size: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub small_size: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub monospace_size: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub item_spacing: Option<[u8; 2]>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub button_padding: Option<[u8; 2]>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub control_height: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub widget_radius: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub window_radius: Option<u8>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub menu_radius: Option<u8>,
}

pub fn parse_color(value: &str) -> Result<[u8; 4], Error> {
	let bytes = value.as_bytes();
	if !matches!(bytes.len(), 7 | 9)
		|| bytes[0] != b'#'
		|| !bytes[1..].iter().all(u8::is_ascii_hexdigit)
	{
		return Err(Error::Invalid);
	}
	let mut rgba = [0, 0, 0, 255];
	for (i, pair) in bytes[1..].as_chunks::<2>().0.iter().enumerate() {
		let digit = |b: u8| {
			if b.is_ascii_digit() {
				b - b'0'
			} else {
				b.to_ascii_lowercase() - b'a' + 10
			}
		};
		rgba[i] = digit(pair[0]) * 16 + digit(pair[1]);
	}
	Ok(rgba)
}

impl Theme {
	/// Apply only explicitly supplied values, preserving the underlying theme's other tokens.
	pub fn overlay(&mut self, other: &Self) {
		for (base, overlay) in [
			(&mut self.light, &other.light),
			(&mut self.dark, &other.dark),
		] {
			base.colors.extend(overlay.colors.clone());
			base.background = overlay.background.or(base.background);
			if overlay.backdrop.is_some() {
				base.backdrop.clone_from(&overlay.backdrop);
			}
		}
		self.style.body_size = other.style.body_size.or(self.style.body_size);
		self.style.transparency_blur = other
			.style
			.transparency_blur
			.or(self.style.transparency_blur);
		self.style.transparency = other.style.transparency.or(self.style.transparency);
		self.style.blur = other.style.blur.or(self.style.blur);
		self.style.heading_size = other.style.heading_size.or(self.style.heading_size);
		self.style.button_size = other.style.button_size.or(self.style.button_size);
		self.style.icon_size = other.style.icon_size.or(self.style.icon_size);
		self.style.small_size = other.style.small_size.or(self.style.small_size);
		self.style.monospace_size = other.style.monospace_size.or(self.style.monospace_size);
		self.style.item_spacing = other.style.item_spacing.or(self.style.item_spacing);
		self.style.button_padding = other.style.button_padding.or(self.style.button_padding);
		self.style.control_height = other.style.control_height.or(self.style.control_height);
		self.style.widget_radius = other.style.widget_radius.or(self.style.widget_radius);
		self.style.window_radius = other.style.window_radius.or(self.style.window_radius);
		self.style.menu_radius = other.style.menu_radius.or(self.style.menu_radius);
	}

	pub fn validate(&self) -> Result<(), Error> {
		let style = self.style;
		if [style.transparency, style.blur]
			.into_iter()
			.flatten()
			.any(|value| value > 100)
			|| ![
				style.body_size,
				style.button_size,
				style.small_size,
				style.monospace_size,
			]
			.into_iter()
			.flatten()
			.all(|size| (10..=28).contains(&size))
			|| style
				.heading_size
				.is_some_and(|size| !(12..=40).contains(&size))
			|| style
				.icon_size
				.is_some_and(|size| !(16..=24).contains(&size))
			|| style
				.control_height
				.is_some_and(|size| !(24..=56).contains(&size))
			|| style
				.item_spacing
				.is_some_and(|sizes| sizes.into_iter().any(|size| size > 24))
			|| style
				.button_padding
				.is_some_and(|sizes| sizes.into_iter().any(|size| size > 24))
			|| [style.widget_radius, style.window_radius, style.menu_radius]
				.into_iter()
				.flatten()
				.any(|size| size > 24)
		{
			return Err(Error::Invalid);
		}
		const NAMES: &[&str] = &[
			"base",
			"sidebar",
			"chat",
			"raised",
			"hover",
			"selected",
			"border",
			"text_strong",
			"text",
			"muted",
			"link",
			"accent",
			"accent_text",
			"positive",
			"warning",
			"danger",
			"mention_bg",
			"mention_text",
		];
		for palette in [&self.light, &self.dark] {
			if palette.background.is_some_and(|background| {
				background.opacity > 100
					|| background.sections.is_some_and(|s| {
						[
							s.top_bar,
							s.server_list,
							s.channel_list,
							s.message_list,
							s.member_list,
							s.composer,
						]
						.into_iter()
						.any(|opacity| opacity > 100)
					})
			}) {
				return Err(Error::Invalid);
			}
			if palette.colors.len() > NAMES.len() {
				return Err(Error::Limit);
			}
			for (name, color) in &palette.colors {
				if !NAMES.contains(&name.as_str()) {
					return Err(Error::Invalid);
				}
				parse_color(color)?;
			}
			if let Some(stops) = &palette.backdrop {
				for color in stops {
					parse_color(color)?;
				}
			}
		}
		Ok(())
	}
}
