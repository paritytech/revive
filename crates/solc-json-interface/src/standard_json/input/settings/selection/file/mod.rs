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
    #[serde(default, rename = "", skip_serializing_if = "HashSet::is_empty")]
    pub per_file: HashSet<SelectionFlag>,
    /// The per-contract output selections.
    #[serde(default, rename = "*", skip_serializing_if = "HashSet::is_empty")]
    pub per_contract: HashSet<SelectionFlag>,
}

impl File {
    /// A shortcut constructor.
    pub fn new(flags: Vec<SelectionFlag>) -> Self {
        let mut per_file = HashSet::new();
        let mut per_contract = HashSet::new();
        for flag in flags.into_iter() {
            match flag {
                SelectionFlag::AST => {
                    per_file.insert(SelectionFlag::AST);
                }
                flag => {
                    per_contract.insert(flag);
                }
            }
        }
        Self {
            per_file,
            per_contract,
        }
    }

    /// Creates the selection required for test compilation (includes EVM bytecode).
    pub fn new_required_for_tests() -> Self {
        Self {
            per_file: HashSet::from_iter([SelectionFlag::AST]),
            per_contract: HashSet::from_iter([
                SelectionFlag::EVMBC,
                SelectionFlag::EVMDBC,
                SelectionFlag::MethodIdentifiers,
                SelectionFlag::Metadata,
                SelectionFlag::Yul,
            ]),
        }
    }

    /// Extends the output selection with another one.
    pub fn extend(&mut self, other: Self) -> &mut Self {
        self.per_file.extend(other.per_file);
        self.per_contract.extend(other.per_contract);
        self
    }

    /// Returns flags that were not explicitly requested by the user.
    ///
    /// These flags are used to prune JSON output before returning it,
    /// removing any output that was automatically added but not requested.
    pub fn selection_to_prune(&self) -> Self {
        let unset_per_file = SelectionFlag::all()
            .iter()
            .copied()
            .filter(|flag| !self.per_file.contains(flag))
            .collect();

        let requests_evm = self.contains(SelectionFlag::EVM);
        let evm_children = SelectionFlag::evm_children();
        let requests_evm_child = self.contains_any(evm_children);

        let unset_per_contract: HashSet<_> = SelectionFlag::all()
            .iter()
            .copied()
            .filter(|flag| {
                // Never prune EVM children when the EVM parent is requested.
                if requests_evm && evm_children.contains(flag) {
                    return false;
                }
                // Never prune the EVM parent when any of its children are requested.
                if requests_evm_child && *flag == SelectionFlag::EVM {
                    return false;
                }
                !self.per_contract.contains(flag)
            })
            .collect();

        Self {
            per_file: unset_per_file,
            per_contract: unset_per_contract,
        }
    }

    /// Checks whether the `flag` is requested.
    pub fn contains(&self, flag: SelectionFlag) -> bool {
        match flag {
            SelectionFlag::AST => self.per_file.contains(&flag),
            _ => self.per_contract.contains(&flag),
        }
    }

    /// Checks whether any of the `flags` is requested.
    pub fn contains_any(&self, flags: &[SelectionFlag]) -> bool {
        flags.iter().any(|&flag| self.contains(flag))
    }

    /// Checks whether the selection is empty.
    pub fn is_empty(&self) -> bool {
        self.per_file.is_empty() && self.per_contract.is_empty()
    }
}
