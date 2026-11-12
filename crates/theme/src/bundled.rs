//! Ready-made themes that ship with DTEW Cord. They are custom theme files compiled into the
//! app, so they can be picked, exported or copied into the editor like the user's own.
use crate::{CustomTheme, Entry};
use std::sync::OnceLock;

/// Prefix of a ready-made theme's id and saved selection, e.g. `theme:latte`. Library ids can't
/// contain `:`, so the two never collide.
pub const BUNDLED_PREFIX: &str = "theme:";

const FILES: [(&str, &str); 9] = [
	("black", include_str!("../themes/black.json")),
	("forest", include_str!("../themes/forest.json")),
	("golden", include_str!("../themes/golden.json")),
	("latte", include_str!("../themes/latte.json")),
	("midnight", include_str!("../themes/midnight.json")),
	("obsidian", include_str!("../themes/obsidian.json")),
	("ocean", include_str!("../themes/ocean.json")),
	("rose", include_str!("../themes/rose.json")),
	("teal", include_str!("../themes/teal.json")),
];

/// Every ready-made theme, sorted by name.
pub fn bundled() -> &'static [Entry] {
	static THEMES: OnceLock<Vec<Entry>> = OnceLock::new();
	THEMES.get_or_init(|| {
		let mut themes: Vec<_> = FILES
			.iter()
			.filter_map(|(slug, json)| {
				Some(Entry {
					id: format!("{BUNDLED_PREFIX}{slug}"),
					theme: CustomTheme::from_json(json).ok()?,
				})
			})
			.collect();
		themes.sort_by(|a, b| a.theme.name.cmp(&b.theme.name));
		themes
	})
}

/// A ready-made theme by its id (`theme:latte`) or bare name (`latte`).
pub fn find_bundled(key: &str) -> Option<&'static Entry> {
	let slug = key.strip_prefix(BUNDLED_PREFIX).unwrap_or(key);
	bundled()
		.iter()
		.find(|entry| entry.id[BUNDLED_PREFIX.len()..] == *slug)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Token;

	#[test]
	fn every_ready_made_theme_parses_and_is_readable() {
		assert_eq!(bundled().len(), FILES.len());
		for entry in bundled() {
			assert!(entry.id.len() <= 32, "{}", entry.id);
			for dark in [true, false] {
				let tokens = entry.theme.tokens(dark);
				let chat = tokens.get(Token::Chat);
				assert!(
					tokens.get(Token::Text).contrast(chat) >= 4.5,
					"{} dark={dark}",
					entry.id
				);
				assert!(
					tokens
						.get(Token::AccentText)
						.contrast(tokens.get(Token::Accent))
						>= 3.0,
					"{} dark={dark}",
					entry.id
				);
			}
		}
		assert_eq!(find_bundled("latte").unwrap().theme.name, "Latte");
		assert_eq!(find_bundled("theme:black").unwrap().theme.name, "Black");
		assert!(find_bundled("standard").is_none());
	}
}
