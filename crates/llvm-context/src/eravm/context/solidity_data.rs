//!
//! The LLVM IR generator Solidity data.
//!

use std::collections::BTreeMap;

///
/// The LLVM IR generator Solidity data.
///
/// Describes some data that is only relevant to Solidity.
///
#[derive(Debug, Default)]
pub struct SolidityData {
    /// The immutables identifier-to-offset mapping. Is only used by Solidity due to
    /// the arbitrariness of its identifiers.
    immutables: BTreeMap<String, usize>,
}

impl SolidityData {
    ///
    /// A shortcut constructor.
    ///
    pub fn new() -> Self {
        Self::default()
    }

    ///
    /// Returns the current number of immutables values in the contract.
    ///
    pub fn immutables_size(&self) -> usize {
        self.immutables.len() * revive_common::BYTE_LENGTH_FIELD
    }

    ///
    /// Allocates memory for an immutable value in the auxiliary heap.
    ///
    /// If the identifier is already known, just returns its offset.
    ///
    pub fn allocate_immutable(&mut self, identifier: &str) -> usize {
        let number_of_elements = self.immutables.len();
        let new_offset = number_of_elements * revive_common::BYTE_LENGTH_FIELD;
        *self
            .immutables
            .entry(identifier.to_owned())
            .or_insert(new_offset)
    }

    ///
    /// Gets the offset of the immutable value.
    ///
    /// If the value is not yet allocated, then it is done forcibly.
    ///
    pub fn get_or_allocate_immutable(&mut self, identifier: &str) -> usize {
        match self.immutables.get(identifier).copied() {
            Some(offset) => offset,
            None => self.allocate_immutable(identifier),
        }
    }
}
