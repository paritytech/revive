//! The `solc --standard-json` output selection.

pub mod file;

use serde::Deserialize;
use serde::Serialize;

use self::file::File as FileSelection;

/// The `solc --standard-json` output selection.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Selection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(rename = "*", skip_serializing_if = "Option::is_none")]
    pub all: Option<FileSelection>,
}

impl Selection {
    /// Creates the selection required by our compilation process.
    pub fn new_required() -> Self {
        Self {
            all: Some(FileSelection::new_required()),
        }
    }

    /// Extends the user's output selection with flag required by our compilation process.
    pub fn extend_with_required(&mut self) -> &mut Self {
        self.all
            .get_or_insert_with(FileSelection::new_required)
            .extend_with_required();
        self
    }
}
