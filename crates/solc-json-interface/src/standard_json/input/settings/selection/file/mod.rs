//! The `solc --standard-json` output file selection.

pub mod flag;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use self::flag::Flag as SelectionFlag;

/// The `solc --standard-json` output file selection.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct File {
    /// The per-file output selections.
    #[serde(rename = "", skip_serializing_if = "Option::is_none")]
    pub per_file: Option<HashSet<SelectionFlag>>,
    /// The per-contract output selections.
    #[serde(rename = "*", skip_serializing_if = "Option::is_none")]
    pub per_contract: Option<HashSet<SelectionFlag>>,
}

impl File {
    /// Creates the selection required for production compilation (excludes EVM bytecode).
    pub fn new_required() -> Self {
        Self {
            per_file: Some(HashSet::from_iter([SelectionFlag::AST])),
            per_contract: Some(HashSet::from_iter([
                SelectionFlag::MethodIdentifiers,
                SelectionFlag::Metadata,
                SelectionFlag::Yul,
            ])),
        }
    }

    /// Creates the selection required for test compilation (includes EVM bytecode).
    pub fn new_required_for_tests() -> Self {
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

        self.per_file
            .get_or_insert_with(HashSet::default)
            .extend(required.per_file.unwrap_or_default());
        self.per_contract
            .get_or_insert_with(HashSet::default)
            .extend(required.per_contract.unwrap_or_default());

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_excludes_evm_bytecode() {
        let selection = File::new_required();
        let per_contract = selection.per_contract.unwrap();

        // Production should NOT include EVM bytecode flags
        assert!(!per_contract.contains(&SelectionFlag::EVMBC));
        assert!(!per_contract.contains(&SelectionFlag::EVMDBC));

        // But should include other required flags
        assert!(per_contract.contains(&SelectionFlag::MethodIdentifiers));
        assert!(per_contract.contains(&SelectionFlag::Metadata));
        assert!(per_contract.contains(&SelectionFlag::Yul));
    }

    #[test]
    fn tests_include_evm_bytecode() {
        let selection = File::new_required_for_tests();
        let per_contract = selection.per_contract.unwrap();

        // Tests should include EVM bytecode flags
        assert!(per_contract.contains(&SelectionFlag::EVMBC));
        assert!(per_contract.contains(&SelectionFlag::EVMDBC));

        // And should also include other required flags
        assert!(per_contract.contains(&SelectionFlag::MethodIdentifiers));
        assert!(per_contract.contains(&SelectionFlag::Metadata));
        assert!(per_contract.contains(&SelectionFlag::Yul));
    }
}
