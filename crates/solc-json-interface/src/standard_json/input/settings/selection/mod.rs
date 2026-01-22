//! The `solc --standard-json` output selection.

pub mod file;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use self::file::flag::Flag;
use self::file::File as FileSelection;

/// The `solc --standard-json` per-file output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct PerFileSelection {
    /// Individual file selection configuration, required for foundry.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub files: BTreeMap<String, FileSelection>,
}

impl PerFileSelection {
    /// Extends the output selection with another one.
    pub fn extend(&mut self, other: Self) {
        for (entry, file) in other.files {
            self.files
                .entry(entry)
                .and_modify(|v| {
                    v.extend(file.clone());
                })
                .or_insert(file);
        }
    }

    /// Returns flags that are going to be automatically added by the compiler,
    /// but were not explicitly requested by the user.
    ///
    /// Afterwards, the flags are used to prune JSON output before returning it.
    pub fn selection_to_prune(&self) -> Self {
        let files = self
            .files
            .iter()
            .map(|(k, v)| (k.to_owned(), v.selection_to_prune()))
            .collect();
        Self { files }
    }

    /// Checks whether `path` contains the `flag` or `None` if there is no selection for `path`.
    pub fn contains(&self, path: &String, flag: &Flag) -> Option<bool> {
        if let Some(file) = self.files.get(path) {
            return Some(file.contains(flag));
        };
        None
    }

    /// Checks whether any of the `flags` is selected in any of the files.
    pub fn contains_any(&self, flags: &[&Flag]) -> bool {
        self.files.values().any(|file| file.contains_any(flags))
    }

    /// Checks whether this is the empty per file selection.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Removes unneeded selections.
    pub fn retain(&mut self) {
        for file in self.files.values_mut() {
            file.per_contract.retain(Flag::is_required_for_codegen);
            file.per_file.retain(Flag::is_required_for_codegen);
        }
    }
}

/// The `solc --standard-json` output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Selection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(default, rename = "*", skip_serializing_if = "FileSelection::is_empty")]
    pub all: FileSelection,
    /// Individual file selection configuration, required for foundry.
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
    pub fn new_required_for_codegen() -> Self {
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

    /// Returns flags that were not explicitly requested by the user.
    ///
    /// These flags are used to prune JSON output before returning it,
    /// removing any output that was automatically added but not requested.
    pub fn selection_to_prune(&self) -> Self {
        Self {
            all: self.all.selection_to_prune(),
            files: self.files.selection_to_prune(),
        }
    }

    /// Checks whether the `flag` is requested.
    pub fn contains(&self, path: &String, flag: &Flag) -> bool {
        self.files
            .contains(path, flag)
            .unwrap_or(self.all.contains(flag))
    }

    /// Checks whether any of the `flags` is requested in any of the files.
    pub fn contains_any(&self, flags: &[&Flag]) -> bool {
        self.all.contains_any(flags) || self.files.contains_any(flags)
    }

    /// Checks whether code generation is requested.
    pub fn requests_codegen(&self) -> bool {
        self.contains_any(&[&Flag::EVM, &Flag::EVMBC, &Flag::EVMDBC, &Flag::Assembly])
    }

    /// Removes unneeded selections.
    pub fn retain(&mut self) {
        self.all.per_file.retain(Flag::is_required_for_codegen);
        self.all.per_contract.retain(Flag::is_required_for_codegen);
        self.files.retain();
    }
}
