//!
//! The LLVM IR generator Vyper data.
//!

///
/// The LLVM IR generator Vyper data.
///
/// Describes some data that is only relevant to Vyper.
///
#[derive(Debug)]
pub struct VyperData {
    /// The immutables size tracker. Stores the size in bytes.
    /// Does not take into account the size of the indexes.
    immutables_size: usize,
    /// Whether the contract forwarder has been used.
    is_forwarder_used: bool,
}

impl VyperData {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(immutables_size: usize, is_forwarder_used: bool) -> Self {
        Self {
            immutables_size,
            is_forwarder_used,
        }
    }

    ///
    /// Returns the size of the immutables data of the contract.
    ///
    pub fn immutables_size(&self) -> usize {
        self.immutables_size
    }

    ///
    /// Sets the forwarder usage flag.
    ///
    pub fn set_is_forwarder_used(&mut self) {
        self.is_forwarder_used = true;
    }

    ///
    /// Returns the forwarder usage flag.
    ///
    pub fn is_forwarder_used(&self) -> bool {
        self.is_forwarder_used
    }
}
