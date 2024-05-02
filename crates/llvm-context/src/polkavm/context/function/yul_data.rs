//! The LLVM function Yul data.

use std::collections::HashMap;

use num::BigUint;

/// The LLVM function Yul data.
/// Describes some data that is only relevant to Yul.
#[derive(Debug)]
pub struct YulData {
    /// The constants saved to variables. Used for peculiar cases like call simulation.
    /// It is a partial implementation of the constant propagation.
    constants: HashMap<String, BigUint>,
}

impl Default for YulData {
    fn default() -> Self {
        Self {
            constants: HashMap::with_capacity(Self::CONSTANTS_HASHMAP_INITIAL_CAPACITY),
        }
    }
}

impl YulData {
    /// The constants hashmap default capacity.
    const CONSTANTS_HASHMAP_INITIAL_CAPACITY: usize = 16;

    /// A shortcut constructor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a constant if it has been saved.
    pub fn get_constant(&self, name: &str) -> Option<BigUint> {
        self.constants.get(name).cloned()
    }

    /// Saves a constant detected with the partial constant propagation.
    pub fn insert_constant(&mut self, name: String, value: BigUint) {
        self.constants.insert(name, value);
    }
}
