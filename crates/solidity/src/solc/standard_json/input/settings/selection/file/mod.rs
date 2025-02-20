//! The `solc --standard-json` output file selection.

pub mod flag;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use self::flag::Flag as SelectionFlag;

/// The `solc --standard-json` output file selection.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct File {
    /// The per-file output selections.
    #[serde(rename = "", skip_serializing_if = "Option::is_none")]
    pub per_file: Option<HashSet<SelectionFlag>>,
    /// The per-contract output selections.
    #[serde(rename = "*", skip_serializing_if = "Option::is_none")]
    pub per_contract: Option<HashSet<SelectionFlag>>,
}

impl File {
    /// Creates the selection required by our compilation process.
    pub fn new_required() -> Self {
        Self {
            per_file: Some(HashSet::from_iter([SelectionFlag::AST])),
            per_contract: Some(HashSet::from_iter([
                SelectionFlag::EVMBC,
                SelectionFlag::EVMDBC,
                SelectionFlag::MethodIdentifiers,
                SelectionFlag::Metadata,
                SelectionFlag::Yul,
            ])),
        }
    }

    /// Extends the user's output selection with flag required by our compilation process.
    pub fn extend_with_required(&mut self) -> &mut Self {
        let required = Self::new_required();

        let set = self.per_file.get_or_insert_with(HashSet::default);
        set.remove(&SelectionFlag::Unknown);
        set.extend(required.per_file.unwrap_or_default());

        let set = self.per_contract.get_or_insert_with(HashSet::default);
        set.remove(&SelectionFlag::Unknown);
        set.extend(required.per_contract.unwrap_or_default());

        self
    }
}
