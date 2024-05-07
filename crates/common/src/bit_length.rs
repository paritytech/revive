//! The common sizes in bits.

/// The `bool` type bit-length.
pub const BIT_LENGTH_BOOLEAN: usize = 1;

/// The `u8` type or byte bit-length.
pub const BIT_LENGTH_BYTE: usize = 8;

/// The x86 word type (usually `u32`) bit-length.
pub const BIT_LENGTH_X32: usize = crate::byte_length::BYTE_LENGTH_X32 * BIT_LENGTH_BYTE;

/// The x86_64 word type (usually `u64`) bit-length.
pub const BIT_LENGTH_X64: usize = crate::byte_length::BYTE_LENGTH_X64 * BIT_LENGTH_BYTE;

/// The ETH address (usually `u160`) bit-length.
pub const BIT_LENGTH_ETH_ADDRESS: usize =
    crate::byte_length::BYTE_LENGTH_ETH_ADDRESS * BIT_LENGTH_BYTE;

/// The VM word (usually `u256` or `i256`) bit-length.
pub const BIT_LENGTH_WORD: usize = crate::byte_length::BYTE_LENGTH_WORD * BIT_LENGTH_BYTE;

/// Bit length of the runtime value type.
pub const BIT_LENGTH_VALUE: usize = crate::byte_length::BYTE_LENGTH_VALUE * BIT_LENGTH_BYTE;

/// Bit length of thre runimte block number type.
pub const BIT_LENGTH_BLOCK_NUMBER: usize =
    crate::byte_length::BYTE_LENGTH_BLOCK_NUMBER * BIT_LENGTH_BYTE;

/// Bit length of thre runimte block timestamp type.
pub const BIT_LENGTH_BLOCK_TIMESTAMP: usize =
    crate::byte_length::BYTE_LENGTH_BLOCK_TIMESTAMP * BIT_LENGTH_BYTE;
