//! The `solc --standard-json` output selection.

pub mod file;

use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

use self::file::flag::Flag;
use self::file::File as FileSelection;

/// The `solc --standard-json` per-file output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
struct PerFileSelection {
    #[serde(skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub files: BTreeMap<String, FileSelection>,
}

impl PerFileSelection {
    fn extend(&mut self, other: Self) {
        for (entry, file) in other.files {
            self.files
                .entry(entry)
                .and_modify(|v| {
                    v.extend(file.clone());
                })
                .or_insert(file);
        }
    }

    fn selection_to_prune(&self) -> Self {
        let files = self
            .files
            .iter()
            .map(|(k, v)| (k.to_owned(), v.selection_to_prune()))
            .collect();
        Self { files }
    }

    fn contains(&self, path: &String, flag: &Flag) -> Option<bool> {
        if let Some(file) = self.files.get(path) {
            return Some(file.contains(flag));
        };
        None
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// The `solc --standard-json` output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Selection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(default, rename = "*", skip_serializing_if = "FileSelection::is_empty")]
    pub all: FileSelection,
    #[serde(skip_serializing_if = "PerFileSelection::is_empty", flatten)]
    files: PerFileSelection,
}

impl Selection {
    /// Creates the selection with arbitrary flags.
    pub fn new(flags: Vec<Flag>) -> Self {
        Self {
            all: FileSelection::new(flags),
            files: Default::default(),
        }
    }

    /// Creates the selection required by our compilation process.
    pub fn new_required() -> Self {
        Self::new(vec![
            Flag::AST,
            Flag::MethodIdentifiers,
            Flag::Metadata,
            Flag::Yul,
        ])
    }

    /// Creates the selection required for test compilation (includes EVM bytecode).
    pub fn new_required_for_tests() -> Self {
        Self {
            all: FileSelection::new_required_for_tests(),
            files: Default::default(),
        }
    }

    /// Creates the selection required by Yul validation process.
    pub fn new_yul_validation() -> Self {
        Self::new(vec![Flag::EVM])
    }

    /// Extends the output selection with another one.
    pub fn extend(&mut self, other: Self) -> &mut Self {
        self.all.extend(other.all);
        self.files.extend(other.files);
        self
    }

    /// Returns flags that are going to be automatically added by the compiler,
    /// but were not explicitly requested by the user.
    ///
    /// Afterwards, the flags are used to prune JSON output before returning it.
    pub fn selection_to_prune(&self) -> Self {
        Self {
            all: self.all.selection_to_prune(),
            files: self.files.selection_to_prune(),
        }
    }

    /// Whether the flag is requested.
    pub fn contains(&self, path: &String, flag: &Flag) -> bool {
        self.files
            .contains(path, flag)
            .unwrap_or(self.all.contains(flag))
    }
}
