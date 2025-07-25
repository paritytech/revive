//! The `solc --standard-json` output selection.

pub mod file;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use self::file::File as FileSelection;

/// The `solc --standard-json` output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Selection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(rename = "*", skip_serializing_if = "Option::is_none")]
    pub all: Option<FileSelection>,

    #[serde(skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub files: BTreeMap<String, FileSelection>,
}

impl Selection {
    /// Creates the selection required by our compilation process.
    pub fn new_required() -> Self {
        Self {
            all: Some(FileSelection::new_required()),
            files: BTreeMap::new(),
        }
    }

    /// Creates the selection required for test compilation (includes EVM bytecode).
    pub fn new_required_for_tests() -> Self {
        Self {
            all: Some(FileSelection::new_required_for_tests()),
            files: BTreeMap::new(),
        }
    }

    /// Extends the user's output selection with flag required by our compilation process.
    pub fn extend_with_required(&mut self) -> &mut Self {
        self.all
            .get_or_insert_with(FileSelection::new_required)
            .extend_with_required();
        for (_, v) in self.files.iter_mut() {
            v.extend_with_required();
        }
        self
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use crate::SolcStandardJsonInputSettingsSelectionFile;

    use super::Selection;

    #[test]
    fn per_file() {
        let init = Selection {
            all: None,
            files: BTreeMap::from([(
                "Test".to_owned(),
                SolcStandardJsonInputSettingsSelectionFile::new_required(),
            )]),
        };

        let deser = serde_json::to_string(&init)
            .and_then(|string| serde_json::from_str(&string))
            .unwrap();

        assert_eq!(init, deser)
    }

    #[test]
    fn all() {
        let init = Selection {
            all: Some(SolcStandardJsonInputSettingsSelectionFile::new_required()),
            files: BTreeMap::new(),
        };

        let deser = serde_json::to_string(&init)
            .and_then(|string| serde_json::from_str(&string))
            .unwrap();

        assert_eq!(init, deser)
    }

    #[test]
    fn all_and_override() {
        let init = Selection {
            all: Some(SolcStandardJsonInputSettingsSelectionFile::new_required()),
            files: BTreeMap::from([(
                "Test".to_owned(),
                SolcStandardJsonInputSettingsSelectionFile::new_required(),
            )]),
        };

        let deser = serde_json::to_string(&init)
            .and_then(|string| serde_json::from_str(&string))
            .unwrap();

        assert_eq!(init, deser)
    }
}
