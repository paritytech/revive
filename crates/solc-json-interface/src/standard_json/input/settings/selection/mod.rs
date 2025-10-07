//! The `solc --standard-json` output selection.

pub mod file;

use serde::Deserialize;
use serde::Serialize;

use self::file::flag::Flag;
use self::file::File as FileSelection;

/// The `solc --standard-json` output selection.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Selection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(default, rename = "*", skip_serializing_if = "FileSelection::is_empty")]
    pub all: FileSelection,
}

impl Selection {
    /// Creates the selection with arbitrary flags.
    pub fn new(flags: Vec<Flag>) -> Self {
        Self {
            all: FileSelection::new(flags),
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
        }
    }

    /// Creates the selection required by Yul validation process.
    pub fn new_yul_validation() -> Self {
        Self::new(vec![Flag::EVM])
    }

    /// Extends the output selection with another one.
    pub fn extend(&mut self, other: Self) -> &mut Self {
        self.all.extend(other.all);
        self
    }

    /// Returns flags that are going to be automatically added by the compiler,
    /// but were not explicitly requested by the user.
    ///
    /// Afterwards, the flags are used to prune JSON output before returning it.
    pub fn selection_to_prune(&self) -> Self {
        Self {
            all: self.all.selection_to_prune(),
        }
    }

    /// Whether the flag is requested.
    pub fn contains(&self, flag: &Flag) -> bool {
        self.all.contains(flag)
    }
}
