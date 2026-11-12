use crate::{CustomTheme, ThemeError, custom::MAX_FILE_BYTES};
use std::io;
use std::path::{Path, PathBuf};

/// Prefix that marks a custom theme in the saved theme selection, e.g. `custom:midnight-mint`.
pub const SELECTION_PREFIX: &str = "custom:";
/// Longest file id, so a saved selection stays within 32 characters.
pub const MAX_ID: usize = 24;
const EXTENSION: &str = "json";

/// The folder of custom theme files, one `<id>.json` per theme.
#[derive(Clone, Debug)]
pub struct Library {
	dir: PathBuf,
}

/// A theme file that was read successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
	pub id: String,
	pub theme: CustomTheme,
}

/// A file in the folder that could not be used, kept so the UI can say why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
	pub file: PathBuf,
	pub error: String,
}

#[derive(Debug)]
pub enum LibraryError {
	Io(io::Error),
	Theme(ThemeError),
	NotFound,
}

impl std::fmt::Display for LibraryError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LibraryError::Io(error) => error.fmt(f),
			LibraryError::Theme(error) => error.fmt(f),
			LibraryError::NotFound => f.write_str("the theme no longer exists"),
		}
	}
}

impl std::error::Error for LibraryError {}

impl From<io::Error> for LibraryError {
	fn from(error: io::Error) -> Self {
		LibraryError::Io(error)
	}
}

impl From<ThemeError> for LibraryError {
	fn from(error: ThemeError) -> Self {
		LibraryError::Theme(error)
	}
}

/// The saved selection key for a custom theme id.
pub fn selection_key(id: &str) -> String {
	format!("{SELECTION_PREFIX}{id}")
}

/// The custom theme id in a saved selection key, if it names one.
pub fn selection_id(key: &str) -> Option<&str> {
	key.strip_prefix(SELECTION_PREFIX).filter(|id| valid_id(id))
}

fn valid_id(id: &str) -> bool {
	(1..=MAX_ID).contains(&id.len())
		&& id
			.bytes()
			.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
		&& !id.starts_with('-')
		&& !id.ends_with('-')
}

/// Lowercase ASCII letters and digits from `name`, joined by single dashes.
fn slug(name: &str) -> String {
	let mut slug = String::new();
	for word in name
		.split(|c: char| !c.is_ascii_alphanumeric())
		.filter(|word| !word.is_empty())
	{
		if !slug.is_empty() {
			slug.push('-');
		}
		slug.extend(word.chars().map(|c| c.to_ascii_lowercase()));
	}
	slug.truncate(MAX_ID - 3);
	let slug = slug.trim_end_matches('-').to_owned();
	if slug.is_empty() {
		"theme".to_owned()
	} else {
		slug
	}
}

impl Library {
	pub fn new(dir: impl Into<PathBuf>) -> Self {
		Self { dir: dir.into() }
	}

	pub fn dir(&self) -> &Path {
		&self.dir
	}

	fn path(&self, id: &str) -> PathBuf {
		self.dir.join(format!("{id}.{EXTENSION}"))
	}

	/// Every readable theme sorted by name, plus the files that were skipped. A missing
	/// folder is an empty library.
	pub fn load(&self) -> (Vec<Entry>, Vec<Skipped>) {
		let mut entries = Vec::new();
		let mut skipped = Vec::new();
		let Ok(dir) = std::fs::read_dir(&self.dir) else {
			return (entries, skipped);
		};
		for file in dir.flatten() {
			let path = file.path();
			if path.extension().and_then(|ext| ext.to_str()) != Some(EXTENSION) {
				continue;
			}
			let Some(id) = path
				.file_stem()
				.and_then(|stem| stem.to_str())
				.filter(|id| valid_id(id))
				.map(str::to_owned)
			else {
				skipped.push(Skipped {
					file: path,
					error: "the file name must be lowercase letters, digits and dashes".into(),
				});
				continue;
			};
			match read(&path) {
				Ok(theme) => entries.push(Entry { id, theme }),
				Err(error) => skipped.push(Skipped {
					file: path,
					error: error.to_string(),
				}),
			}
		}
		entries.sort_by(|a, b| {
			a.theme
				.name
				.to_lowercase()
				.cmp(&b.theme.name.to_lowercase())
				.then_with(|| a.id.cmp(&b.id))
		});
		(entries, skipped)
	}

	pub fn get(&self, id: &str) -> Result<CustomTheme, LibraryError> {
		if !valid_id(id) {
			return Err(LibraryError::NotFound);
		}
		match read(&self.path(id)) {
			Err(LibraryError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
				Err(LibraryError::NotFound)
			}
			other => other,
		}
	}

	/// Save over `id`, or as a new file named after the theme when `id` is `None`.
	/// Returns the id the theme was saved under.
	pub fn save(&self, id: Option<&str>, theme: &CustomTheme) -> Result<String, LibraryError> {
		theme.validate()?;
		std::fs::create_dir_all(&self.dir)?;
		let id = match id.filter(|id| valid_id(id)) {
			Some(id) => id.to_owned(),
			None => self.free_id(&theme.name),
		};
		// Write beside the target and rename, so a crash never leaves half a theme.
		let path = self.path(&id);
		let temporary = path.with_extension("json.tmp");
		std::fs::write(&temporary, theme.to_json())?;
		std::fs::rename(&temporary, &path)?;
		Ok(id)
	}

	pub fn delete(&self, id: &str) -> Result<(), LibraryError> {
		if !valid_id(id) {
			return Err(LibraryError::NotFound);
		}
		std::fs::remove_file(self.path(id)).map_err(|error| {
			if error.kind() == io::ErrorKind::NotFound {
				LibraryError::NotFound
			} else {
				error.into()
			}
		})
	}

	/// Copy a theme file from anywhere into the library as a new theme.
	pub fn import(&self, file: &Path) -> Result<Entry, LibraryError> {
		let theme = read(file)?;
		let id = self.save(None, &theme)?;
		Ok(Entry { id, theme })
	}

	/// Write a library theme to `file` for sharing.
	pub fn export(&self, id: &str, file: &Path) -> Result<(), LibraryError> {
		let theme = self.get(id)?;
		std::fs::write(file, theme.to_json())?;
		Ok(())
	}

	fn free_id(&self, name: &str) -> String {
		let base = slug(name);
		if !self.path(&base).exists() {
			return base;
		}
		(2..)
			.map(|n| format!("{base}-{n}"))
			.find(|id| !self.path(id).exists())
			.unwrap_or(base)
	}
}

fn read(path: &Path) -> Result<CustomTheme, LibraryError> {
	if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
		return Err(ThemeError::TooLarge.into());
	}
	let text = std::fs::read_to_string(path)?;
	Ok(CustomTheme::from_json(&text)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Builtin, Color, Token};

	struct TempDir(PathBuf);
	impl TempDir {
		fn new(name: &str) -> Self {
			let dir = std::env::temp_dir().join(format!(
				// Debug output of SystemTime contains ':', which Windows paths reject.
				"dtewcord-theme-{name}-{}-{}",
				std::process::id(),
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap_or_default()
					.as_nanos()
			));
			let _ = std::fs::remove_dir_all(&dir);
			Self(dir)
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn slugs_and_selection_keys() {
		assert_eq!(slug("Midnight Mint!"), "midnight-mint");
		assert_eq!(slug("  Ünïcode  "), "n-code");
		assert_eq!(slug("日本"), "theme");
		assert!(slug(&"long ".repeat(20)).len() <= MAX_ID - 3);
		assert_eq!(selection_key("mint"), "custom:mint");
		assert_eq!(selection_id("custom:mint"), Some("mint"));
		assert_eq!(selection_id("custom:../x"), None);
		assert_eq!(selection_id("eclipse"), None);
		assert!(selection_key(&"a".repeat(MAX_ID)).len() <= 32);
	}

	#[test]
	fn save_load_rename_delete() {
		let temp = TempDir::new("crud");
		let library = Library::new(&temp.0);
		assert_eq!(library.load(), (vec![], vec![]));

		let mut theme = CustomTheme::new("Mint", Builtin::Slate);
		theme
			.dark
			.colors
			.insert(Token::Accent, Color::rgb(0x2fd39a));
		let id = library.save(None, &theme).unwrap();
		assert_eq!(id, "mint");
		// A second theme with the same name gets its own file.
		assert_eq!(library.save(None, &theme).unwrap(), "mint-2");

		theme.name = "Mint Renamed".into();
		assert_eq!(library.save(Some("mint"), &theme).unwrap(), "mint");
		assert_eq!(library.get("mint").unwrap(), theme);

		let (entries, skipped) = library.load();
		assert!(skipped.is_empty());
		let names: Vec<_> = entries.iter().map(|e| e.theme.name.as_str()).collect();
		assert_eq!(names, ["Mint", "Mint Renamed"]);

		library.delete("mint-2").unwrap();
		assert!(matches!(library.get("mint-2"), Err(LibraryError::NotFound)));
		assert!(matches!(
			library.delete("mint-2"),
			Err(LibraryError::NotFound)
		));
	}

	#[test]
	fn import_export_and_bad_files() {
		let temp = TempDir::new("io");
		let library = Library::new(temp.0.join("themes"));
		std::fs::create_dir_all(library.dir()).unwrap();
		std::fs::write(library.dir().join("broken.json"), "{").unwrap();
		std::fs::write(library.dir().join("Bad Name.json"), "{}").unwrap();
		std::fs::write(library.dir().join("notes.txt"), "ignored").unwrap();

		let outside = temp.0.join("shared.json");
		std::fs::write(
			&outside,
			CustomTheme::new("Shared", Builtin::Ember).to_json(),
		)
		.unwrap();
		let entry = library.import(&outside).unwrap();
		assert_eq!(entry.id, "shared");

		let exported = temp.0.join("out.json");
		library.export("shared", &exported).unwrap();
		assert_eq!(
			CustomTheme::from_json(&std::fs::read_to_string(exported).unwrap()).unwrap(),
			entry.theme
		);

		let (entries, skipped) = library.load();
		assert_eq!(entries.len(), 1);
		assert_eq!(skipped.len(), 2);

		std::fs::write(&outside, r#"{"format":9,"name":"Future"}"#).unwrap();
		assert!(matches!(
			library.import(&outside),
			Err(LibraryError::Theme(ThemeError::UnsupportedFormat(9)))
		));
	}
}
