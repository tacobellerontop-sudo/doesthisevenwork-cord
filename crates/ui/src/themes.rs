//! DTEW Cord's theme picker: built-in themes, the user's custom theme files and their editor.
//!
//! Theme files are a few kilobytes and are only read or written when the user acts, so the
//! library is used directly from the UI thread. Native file dialogs belong to the host, which
//! answers [`Themes::file_request`] with [`Themes::import_file`] or [`Themes::export_file`].
use crate::{design, dialog, i18n::translate};
use std::path::{Path, PathBuf};
use theme::{Builtin, Color, CustomTheme, Library, Token};

/// A native file dialog the host should open for the theme picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileRequest {
	/// Choose a theme file to add to the library.
	Import,
	/// Choose where to save a copy of a library theme. `name` suggests the file name.
	Export { id: String, name: String },
	/// Show the themes folder in the file manager.
	OpenFolder(PathBuf),
}

#[derive(Default)]
pub struct Themes {
	library: Option<Library>,
	entries: Vec<theme::Entry>,
	skipped: Vec<theme::Skipped>,
	/// The custom theme showing, if any; otherwise [`design::variant`] is.
	active: Option<String>,
	editor: Option<Editor>,
	notice: Option<(design::Level, String)>,
	/// Set by the picker; the host opens the dialog and takes the request.
	pub file_request: Option<FileRequest>,
	/// The saved selection changed; the host persists [`Themes::selection_key`].
	pub selection_changed: bool,
}

impl Themes {
	/// Use `dir` as the custom theme folder and read it.
	pub fn set_library(&mut self, dir: PathBuf) {
		self.library = Some(Library::new(dir));
		self.reload();
	}

	pub fn reload(&mut self) {
		(self.entries, self.skipped) = self.library.as_ref().map(Library::load).unwrap_or_default();
	}

	pub fn entries(&self) -> &[theme::Entry] {
		&self.entries
	}

	/// A ready-made theme (`theme:<name>`) or a library theme by its id.
	fn entry(&self, id: &str) -> Option<&theme::Entry> {
		if id.starts_with(theme::BUNDLED_PREFIX) {
			return theme::bundled().iter().find(|entry| entry.id == id);
		}
		self.entries.iter().find(|entry| entry.id == id)
	}

	/// The key the host saves: a built-in key, a ready-made theme's `theme:<name>`, or
	/// `custom:<id>`. `None` is the default theme.
	pub fn selection_key(&self) -> Option<String> {
		match &self.active {
			Some(id) if id.starts_with(theme::BUNDLED_PREFIX) => Some(id.clone()),
			Some(id) => Some(theme::selection_key(id)),
			None => {
				let variant = design::variant();
				(variant != Builtin::Standard).then(|| variant.key().to_owned())
			}
		}
	}

	/// Show the saved selection. Unknown keys and missing files fall back to the default theme.
	pub fn restore(&mut self, ctx: &egui::Context, key: Option<&str>) {
		let id = key.and_then(|key| {
			if key.starts_with(theme::BUNDLED_PREFIX) {
				Some(key)
			} else {
				theme::selection_id(key)
			}
		});
		if let Some(entry) = id.and_then(|id| self.entry(id)).cloned() {
			self.show_custom(Some(&entry));
		} else {
			self.show_custom(None);
			design::set_variant(key.and_then(Builtin::from_key).unwrap_or_default());
		}
		design::apply(ctx);
	}

	pub fn select_builtin(&mut self, ctx: &egui::Context, variant: Builtin) {
		self.show_custom(None);
		design::set_variant(variant);
		design::apply(ctx);
		self.selection_changed = true;
	}

	pub fn select_custom(&mut self, ctx: &egui::Context, id: &str) {
		let Some(entry) = self.entry(id).cloned() else {
			return;
		};
		self.show_custom(Some(&entry));
		design::apply(ctx);
		self.selection_changed = true;
	}

	fn show_custom(&mut self, entry: Option<&theme::Entry>) {
		design::set_custom_theme(entry.map(|entry| &entry.theme));
		self.active = entry.map(|entry| entry.id.clone());
	}

	/// The host's answer to [`FileRequest::Import`]; the imported theme is selected.
	pub fn import_file(&mut self, ctx: &egui::Context, file: &Path) {
		let Some(library) = &self.library else {
			return;
		};
		match library.import(file) {
			Ok(entry) => {
				self.reload();
				self.notice = Some((
					design::Level::Success,
					crate::i18n::translate_args(
						"themes-imported",
						&[("name", entry.theme.name.as_str())],
					),
				));
				self.select_custom(ctx, &entry.id);
			}
			Err(error) => {
				self.notice = Some((
					design::Level::Error,
					crate::i18n::translate_args(
						"themes-import-failed",
						&[("error", &error.to_string())],
					),
				));
			}
		}
	}

	/// The host's answer to [`FileRequest::Export`].
	pub fn export_file(&mut self, id: &str, file: &Path) {
		let result = if let Some(entry) =
			theme::find_bundled(id).filter(|_| id.starts_with(theme::BUNDLED_PREFIX))
		{
			std::fs::write(file, entry.theme.to_json()).map_err(theme::LibraryError::Io)
		} else if let Some(library) = &self.library {
			library.export(id, file)
		} else {
			return;
		};
		self.notice = Some(match result {
			Ok(()) => (design::Level::Success, translate("themes-exported")),
			Err(error) => (
				design::Level::Error,
				crate::i18n::translate_args(
					"themes-export-failed",
					&[("error", &error.to_string())],
				),
			),
		});
	}

	/// The swatch row for Settings › Appearance, with the custom theme actions under it.
	pub fn picker(&mut self, ui: &mut egui::Ui) {
		let dark = ui.visuals().dark_mode;
		let current = design::variant();
		let mut chosen_builtin = None;
		let mut chosen_custom = None;
		design::group(ui, "themes-picker-title", |ui| {
			ui.horizontal_wrapped(|ui| {
				ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
				for variant in Builtin::ALL {
					let selected = self.active.is_none() && variant == current;
					let swatch = design::builtin_colors(dark, variant);
					let label = variant.label();
					if crate::settings::preset_swatch(ui, label, &swatch, selected)
						.on_hover_text(label)
						.clicked() && !selected
					{
						chosen_builtin = Some(variant);
					}
				}
				for entry in theme::bundled().iter().chain(&self.entries) {
					let selected = self.active.as_deref() == Some(entry.id.as_str());
					let swatch = design::custom_preview_colors(dark, &entry.theme);
					let hover = match &entry.theme.author {
						Some(author) => crate::i18n::translate_args(
							"themes-by-author",
							&[("name", &entry.theme.name), ("author", author)],
						),
						None => entry.theme.name.clone(),
					};
					if crate::settings::preset_swatch(ui, &entry.theme.name, &swatch, selected)
						.on_hover_text(hover)
						.clicked() && !selected
					{
						chosen_custom = Some(entry.id.clone());
					}
				}
			});
			design::card_divider(ui);
			ui.horizontal_wrapped(|ui| {
				ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
				if design::button(ui, "themes-new", design::ButtonKind::Outline).clicked() {
					self.open_editor(ui.ctx(), None);
				}
				if let Some(id) = self.active.clone() {
					if design::button(ui, "themes-edit", design::ButtonKind::Outline).clicked() {
						self.open_editor(ui.ctx(), Some(id.clone()));
					}
					if design::button(ui, "themes-export", design::ButtonKind::Outline).clicked()
						&& let Some(entry) = self.entry(&id)
					{
						self.file_request = Some(FileRequest::Export {
							name: entry.theme.name.clone(),
							id,
						});
					}
				}
				if design::button(ui, "themes-import", design::ButtonKind::Outline).clicked() {
					self.file_request = Some(FileRequest::Import);
				}
				if let Some(library) = &self.library
					&& design::text_action(ui, "themes-open-folder").clicked()
				{
					let _ = std::fs::create_dir_all(library.dir());
					self.file_request = Some(FileRequest::OpenFolder(library.dir().to_owned()));
				}
			});
			if let Some((level, text)) = &self.notice {
				ui.add_space(4.0);
				design::notice(ui, *level, text);
			}
			if !self.skipped.is_empty() {
				ui.add_space(4.0);
				let names: Vec<_> = self
					.skipped
					.iter()
					.map(|skipped| {
						let file = skipped
							.file
							.file_name()
							.map(|name| name.to_string_lossy().into_owned())
							.unwrap_or_default();
						format!("{file}: {}", skipped.error)
					})
					.collect();
				design::notice(
					ui,
					design::Level::Warning,
					&crate::i18n::translate_args(
						"themes-skipped-files",
						&[("files", &names.join("\n"))],
					),
				);
			}
		});
		if let Some(variant) = chosen_builtin {
			self.select_builtin(ui.ctx(), variant);
		}
		if let Some(id) = chosen_custom {
			self.select_custom(ui.ctx(), &id);
		}
		self.show_editor(ui.ctx());
	}

	/// Start editing `id`, or a new theme based on what is showing now.
	fn open_editor(&mut self, ctx: &egui::Context, mut id: Option<String>) {
		let dark = ctx.theme() == egui::Theme::Dark;
		// Ready-made themes can't change; editing one starts a copy in the library.
		if let Some(entry) = id
			.as_deref()
			.filter(|id| id.starts_with(theme::BUNDLED_PREFIX))
			.and_then(|id| self.entry(id))
		{
			let mut draft = entry.theme.clone();
			draft.name = crate::i18n::translate_args("themes-copy-name", &[("name", &draft.name)])
				// Fluent wraps arguments in Unicode isolation marks; a saved name shouldn't keep them.
				.replace(['\u{2068}', '\u{2069}'], "");
			draft.author = None;
			id = None;
			self.open_editor_with(ctx, id, draft, dark);
			return;
		}
		let draft = match id.as_deref().and_then(|id| self.entry(id)) {
			Some(entry) => entry.theme.clone(),
			None => {
				let mut draft = match self.active.as_deref().and_then(|id| self.entry(id)) {
					Some(entry) => entry.theme.clone(),
					None => CustomTheme::new(String::new(), design::variant()),
				};
				draft.name = translate("themes-editor-default-name");
				draft.author = None;
				draft
			}
		};
		self.open_editor_with(ctx, id, draft, dark);
	}

	fn open_editor_with(
		&mut self,
		ctx: &egui::Context,
		id: Option<String>,
		draft: CustomTheme,
		dark: bool,
	) {
		self.notice = None;
		self.editor = Some(Editor {
			id,
			draft,
			dark,
			previous: (self.active.clone(), design::variant()),
			hex: None,
			confirm_delete: false,
			error: None,
		});
		self.preview(ctx);
	}

	fn preview(&self, ctx: &egui::Context) {
		if let Some(editor) = &self.editor {
			design::set_custom_theme(Some(&editor.draft));
			design::apply(ctx);
		}
	}

	/// Put back what was showing before the editor opened.
	fn close_editor(&mut self, ctx: &egui::Context) {
		let Some(editor) = self.editor.take() else {
			return;
		};
		let (previous, variant) = editor.previous;
		match previous.and_then(|id| self.entry(&id).cloned()) {
			Some(entry) => self.show_custom(Some(&entry)),
			None => {
				self.show_custom(None);
				design::set_variant(variant);
			}
		}
		design::apply(ctx);
	}

	fn show_editor(&mut self, ctx: &egui::Context) {
		let Some(editor) = &mut self.editor else {
			return;
		};
		let title = if editor.id.is_some() {
			translate("themes-editor-edit-title")
		} else {
			translate("themes-editor-new-title")
		};
		let mut action = None;
		let response = dialog::Dialog::new("theme-editor", title)
			.subtitle(translate("themes-editor-subtitle"))
			.width(560.0)
			.show(ctx, |body| {
				let changed = body.scroll(220.0, |ui| editor.body(ui));
				body.footer(|ui| {
					if editor.confirm_delete {
						if dialog::action(
							ui,
							"themes-editor-delete-confirm",
							dialog::Action::Danger,
						)
						.clicked()
						{
							action = Some(EditorAction::Delete);
						}
						if dialog::action(ui, "themes-editor-keep", dialog::Action::Neutral)
							.clicked()
						{
							editor.confirm_delete = false;
						}
						return;
					}
					if dialog::action(ui, "themes-editor-save", dialog::Action::Primary).clicked() {
						action = Some(EditorAction::Save);
					}
					if dialog::action(ui, "themes-editor-cancel", dialog::Action::Neutral).clicked()
					{
						action = Some(EditorAction::Cancel);
					}
					if editor.id.is_some() {
						ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
							if dialog::action(ui, "themes-editor-delete", dialog::Action::Outline)
								.clicked()
							{
								editor.confirm_delete = true;
							}
						});
					}
				});
				changed
			});
		if response.close && action.is_none() {
			action = Some(EditorAction::Cancel);
		}
		if response.inner {
			self.preview(ctx);
		}
		match action {
			Some(EditorAction::Save) => self.save_editor(ctx),
			Some(EditorAction::Cancel) => self.close_editor(ctx),
			Some(EditorAction::Delete) => self.delete_editor(ctx),
			None => {}
		}
	}

	fn save_editor(&mut self, ctx: &egui::Context) {
		let (Some(library), Some(editor)) = (&self.library, &mut self.editor) else {
			return;
		};
		editor.draft.name = editor.draft.name.trim().to_owned();
		match library.save(editor.id.as_deref(), &editor.draft) {
			Ok(id) => {
				self.editor = None;
				self.reload();
				self.select_custom(ctx, &id);
				self.notice = None;
			}
			Err(error) => editor.error = Some(error.to_string()),
		}
	}

	fn delete_editor(&mut self, ctx: &egui::Context) {
		let (Some(library), Some(editor)) = (&self.library, &mut self.editor) else {
			return;
		};
		let Some(id) = editor.id.clone() else {
			return;
		};
		match library.delete(&id) {
			Ok(()) | Err(theme::LibraryError::NotFound) => {
				let base = editor.draft.base();
				self.editor = None;
				self.reload();
				// The deleted theme was showing; fall back to the built-in it started from.
				self.select_builtin(ctx, base);
			}
			Err(error) => {
				editor.confirm_delete = false;
				editor.error = Some(error.to_string());
			}
		}
	}
}

enum EditorAction {
	Save,
	Cancel,
	Delete,
}

struct Editor {
	/// The library file being edited; `None` until a new theme is first saved.
	id: Option<String>,
	draft: CustomTheme,
	/// Which appearance's colours are being edited.
	dark: bool,
	/// Custom theme id and built-in that were showing before the editor opened.
	previous: (Option<String>, Builtin),
	/// Hex text being typed into one colour row.
	hex: Option<(Token, String)>,
	confirm_delete: bool,
	error: Option<String>,
}

const GROUPS: [(&str, &[Token]); 4] = [
	(
		"themes-group-surfaces",
		&[
			Token::Base,
			Token::Sidebar,
			Token::Chat,
			Token::Raised,
			Token::Hover,
			Token::Selected,
			Token::Border,
		],
	),
	(
		"themes-group-text",
		&[Token::TextStrong, Token::Text, Token::Muted, Token::Link],
	),
	(
		"themes-group-accent",
		&[
			Token::Accent,
			Token::AccentText,
			Token::MentionBg,
			Token::MentionText,
		],
	),
	(
		"themes-group-status",
		&[Token::Positive, Token::Warning, Token::Danger],
	),
];

impl Editor {
	/// Returns whether the draft's colours changed.
	fn body(&mut self, ui: &mut egui::Ui) -> bool {
		let mut changed = false;
		if let Some(error) = &self.error {
			design::notice(ui, design::Level::Error, error);
			ui.add_space(8.0);
		}
		design::card(ui, |ui| {
			design::row(ui, "themes-editor-name", None, |ui| {
				design::input(
					ui,
					egui::TextEdit::singleline(&mut self.draft.name)
						.char_limit(theme::MAX_NAME)
						.hint_text(translate("themes-editor-default-name")),
				);
			});
			design::card_divider(ui);
			design::row(
				ui,
				"themes-editor-base",
				Some("themes-editor-base-description"),
				|ui| {
					let base = self.draft.base();
					design::Select::from_id_salt("theme-editor-base")
						.selected_text(base.label())
						.width(ui.available_width().min(200.0))
						.show_ui(ui, |ui| {
							for candidate in Builtin::ALL {
								if ui
									.selectable_label(candidate == base, candidate.label())
									.clicked() && candidate != base
								{
									self.draft.base = candidate.key().to_owned();
									changed = true;
								}
							}
						});
				},
			);
			design::card_divider(ui);
			design::row(
				ui,
				"themes-editor-appearance",
				Some(if self.dark == (ui.visuals().dark_mode) {
					"themes-editor-appearance-description"
				} else {
					"themes-editor-appearance-not-showing"
				}),
				|ui| {
					let labels = ["themes-editor-dark", "themes-editor-light"];
					if let Some(index) = design::segmented(ui, &labels, usize::from(!self.dark)) {
						self.dark = index == 0;
						self.hex = None;
					}
				},
			);
		});
		let tokens = self.draft.tokens(self.dark);
		for (title, group) in GROUPS {
			ui.add_space(12.0);
			design::group(ui, title, |ui| {
				for (index, token) in group.iter().enumerate() {
					if index > 0 {
						design::card_divider(ui);
					}
					changed |= self.color_row(ui, *token, tokens.get(*token));
				}
			});
		}
		ui.add_space(12.0);
		design::group(ui, "themes-group-backdrop", |ui| {
			changed |= self.backdrop_row(ui, tokens.backdrop);
		});
		let overrides = self.draft.overrides(self.dark);
		if !overrides.is_empty() {
			ui.add_space(8.0);
			if design::text_action(ui, "themes-editor-reset-all").clicked() {
				*self.draft.overrides_mut(self.dark) = theme::Overrides::default();
				self.hex = None;
				changed = true;
			}
		}
		changed
	}

	fn color_row(&mut self, ui: &mut egui::Ui, token: Token, current: Color) -> bool {
		let mut changed = false;
		let overridden = self.draft.overrides(self.dark).colors.contains_key(&token);
		let title = format!("themes-token-{}", token.key().replace('_', "-"));
		let detail = format!("{title}-description");
		design::row(ui, &title, Some(&detail), |ui| {
			ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
				if let Some(color) = color_field(ui, token, current, &mut self.hex) {
					self.draft
						.overrides_mut(self.dark)
						.colors
						.insert(token, color);
					changed = true;
				}
				if overridden && design::text_action(ui, "themes-editor-reset").clicked() {
					self.draft.overrides_mut(self.dark).colors.remove(&token);
					self.hex = None;
					changed = true;
				}
			});
		});
		changed
	}

	fn backdrop_row(&mut self, ui: &mut egui::Ui, current: Option<[Color; 2]>) -> bool {
		let mut changed = false;
		design::row(
			ui,
			"themes-token-backdrop",
			Some("themes-token-backdrop-description"),
			|ui| {
				ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
					match current {
						Some([top, bottom]) => {
							let mut stops = [top, bottom];
							// Right to left: the bottom-right stop first, so the top-left one reads first.
							for slot in [1, 0] {
								let mut rgba = stops[slot].to_array();
								if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
									let [r, g, b, a] = rgba;
									stops[slot] = Color { r, g, b, a };
									changed = true;
								}
							}
							if changed {
								self.draft.overrides_mut(self.dark).backdrop = Some(stops);
							}
							if self.draft.overrides(self.dark).backdrop.is_some()
								&& design::text_action(ui, "themes-editor-reset").clicked()
							{
								self.draft.overrides_mut(self.dark).backdrop = None;
								changed = true;
							}
						}
						None => {
							if design::text_action(ui, "themes-editor-add-backdrop").clicked() {
								let tokens = self.draft.tokens(self.dark);
								self.draft.overrides_mut(self.dark).backdrop = Some([
									tokens.get(Token::Accent).mix(Color::BLACK, 0.4),
									tokens.get(Token::Base).with_alpha(255),
								]);
								changed = true;
							}
						}
					}
				});
			},
		);
		changed
	}
}

/// A colour well and a `#RRGGBB[AA]` field. Returns a new colour once one is picked or typed.
fn color_field(
	ui: &mut egui::Ui,
	token: Token,
	current: Color,
	hex: &mut Option<(Token, String)>,
) -> Option<Color> {
	let mut result = None;
	let mut text = match hex {
		Some((editing, text)) if *editing == token => text.clone(),
		_ => current.to_string(),
	};
	let field = ui.add(
		egui::TextEdit::singleline(&mut text)
			.desired_width(92.0)
			.char_limit(9)
			.font(egui::TextStyle::Monospace),
	);
	if field.changed() {
		if let Ok(color) = Color::parse(&text) {
			result = Some(color);
		}
		*hex = Some((token, text));
	} else if field.lost_focus() && hex.as_ref().is_some_and(|(editing, _)| *editing == token) {
		*hex = None;
	}
	let mut rgba = current.to_array();
	if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
		let [r, g, b, a] = rgba;
		result = Some(Color { r, g, b, a });
		*hex = None;
	}
	result
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ready_made_themes_are_listed_selected_and_saved() {
		let ctx = egui::Context::default();
		let mut themes = Themes::default();
		themes.restore(&ctx, Some("theme:latte"));
		assert!(design::custom_theme_active());
		assert_eq!(themes.selection_key().as_deref(), Some("theme:latte"));
		// A missing ready-made theme falls back to the default theme.
		themes.restore(&ctx, Some("theme:gone"));
		assert!(!design::custom_theme_active());
		assert_eq!(themes.selection_key(), None);
		themes.select_custom(&ctx, "theme:obsidian");
		assert!(themes.selection_changed);
		assert_eq!(themes.selection_key().as_deref(), Some("theme:obsidian"));
		// Editing a ready-made theme starts an unsaved copy instead.
		themes.open_editor(&ctx, Some("theme:obsidian".into()));
		let editor = themes.editor.as_ref().unwrap();
		assert_eq!(editor.id, None);
		// Fluent wraps arguments in Unicode isolation marks.
		assert_eq!(
			editor.draft.name.replace(['\u{2068}', '\u{2069}'], ""),
			"Obsidian copy"
		);
		themes.close_editor(&ctx);
		assert_eq!(themes.selection_key().as_deref(), Some("theme:obsidian"));
		design::set_custom_theme(None);
		design::set_variant(Builtin::Standard);
	}

	#[test]
	fn every_token_is_in_one_editor_group() {
		let mut seen: Vec<Token> = GROUPS
			.iter()
			.flat_map(|(_, tokens)| tokens.iter().copied())
			.collect();
		seen.sort();
		let mut all = Token::ALL.to_vec();
		all.sort();
		assert_eq!(seen, all);
	}

	#[test]
	fn every_editor_string_has_english_text() {
		let mut keys: Vec<String> = [
			"themes-picker-title",
			"themes-new",
			"themes-edit",
			"themes-export",
			"themes-import",
			"themes-open-folder",
			"themes-exported",
			"themes-editor-edit-title",
			"themes-editor-new-title",
			"themes-editor-subtitle",
			"themes-editor-default-name",
			"themes-editor-name",
			"themes-editor-base",
			"themes-editor-base-description",
			"themes-editor-appearance",
			"themes-editor-appearance-description",
			"themes-editor-appearance-not-showing",
			"themes-editor-dark",
			"themes-editor-light",
			"themes-editor-save",
			"themes-editor-cancel",
			"themes-editor-delete",
			"themes-editor-delete-confirm",
			"themes-editor-keep",
			"themes-editor-reset",
			"themes-editor-reset-all",
			"themes-editor-add-backdrop",
			"themes-token-backdrop",
			"themes-token-backdrop-description",
		]
		.map(str::to_owned)
		.to_vec();
		keys.extend(GROUPS.iter().map(|(title, _)| (*title).to_owned()));
		for token in Token::ALL {
			let key = format!("themes-token-{}", token.key().replace('_', "-"));
			keys.push(format!("{key}-description"));
			keys.push(key);
		}
		for key in keys {
			assert!(
				!translate(&key).starts_with("Unknown localization key"),
				"{key}"
			);
		}
		for key in [
			"themes-copy-name",
			"themes-by-author",
			"themes-imported",
			"themes-import-failed",
			"themes-export-failed",
			"themes-skipped-files",
		] {
			let text = crate::i18n::translate_args(
				key,
				&[
					("name", "x"),
					("error", "x"),
					("files", "x"),
					("author", "x"),
				],
			);
			assert!(!text.starts_with("Unknown localization key"), "{key}");
		}
	}
}
