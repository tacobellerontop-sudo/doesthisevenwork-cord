//! DTEW Cord's theming system, shared by the desktop and Slint clients.
//!
//! A theme is a full set of colour [`Tokens`] for the dark and the light appearance.
//! [`Builtin`] themes ship with the app; a [`CustomTheme`] starts from a built-in and changes
//! some of its colours, and is stored as a small JSON file in a [`Library`] folder.
//! The toolkit-specific code in each client only maps tokens onto its own widgets.

mod builtin;
mod bundled;
mod color;
mod custom;
mod library;
mod tokens;

pub use builtin::{Builtin, DARK_DANGER, DEFAULT_ACCENT};
pub use bundled::{BUNDLED_PREFIX, bundled, find_bundled};
pub use color::{Color, ColorError};
pub use custom::{CustomTheme, FORMAT, MAX_FILE_BYTES, MAX_NAME, Overrides, ThemeError};
pub use library::{
	Entry, Library, LibraryError, MAX_ID, SELECTION_PREFIX, Skipped, selection_id, selection_key,
};
pub use tokens::{Token, Tokens};

/// The folder name for custom themes inside the app's data directory.
pub const LIBRARY_DIR: &str = "themes";
