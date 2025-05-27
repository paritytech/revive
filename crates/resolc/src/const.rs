//! Solidity to PolkaVM compiler constants.

/// The default executable name.
pub static DEFAULT_EXECUTABLE_NAME: &str = "resolc";

/// The `keccak256` scratch space offset.
pub const OFFSET_SCRATCH_SPACE: usize = 0;

/// The memory pointer offset.
pub const OFFSET_MEMORY_POINTER: usize = 2 * revive_common::BYTE_LENGTH_WORD;

/// The empty slot offset.
pub const OFFSET_EMPTY_SLOT: usize = 3 * revive_common::BYTE_LENGTH_WORD;

/// The non-reserved memory offset.
pub const OFFSET_NON_RESERVED: usize = 4 * revive_common::BYTE_LENGTH_WORD;
