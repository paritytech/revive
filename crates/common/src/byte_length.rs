//! The common sizes in bytes.

/// The byte-length.
pub const BYTE_LENGTH_BYTE: usize = 1;

/// The x86 word byte-length.
pub const BYTE_LENGTH_X32: usize = 4;

/// Native stack alignment size in bytes
#[cfg(not(feature = "riscv-64"))]
pub const BYTE_LENGTH_STACK_ALIGN: usize = 4;
#[cfg(feature = "riscv-64")]
pub const BYTE_LENGTH_STACK_ALIGN: usize = 8;

/// The x86_64 word byte-length.
pub const BYTE_LENGTH_X64: usize = 8;

/// The ETH address byte-length.
pub const BYTE_LENGTH_ETH_ADDRESS: usize = 20;

/// The word byte-length.
pub const BYTE_LENGTH_WORD: usize = 32;

/// Byte length of the runtime value type.
pub const BYTE_LENGTH_VALUE: usize = 32;

/// Byte length of the runtime block number type.
pub const BYTE_LENGTH_BLOCK_NUMBER: usize = 8;

/// Byte length of the runtime block timestamp type.
pub const BYTE_LENGTH_BLOCK_TIMESTAMP: usize = 8;
