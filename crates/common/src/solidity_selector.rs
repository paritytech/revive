//! Solidity ABI selector words used when emitting reverts.
//!
//! Each constant is the 32-byte EVM memory word that stores the corresponding
//! Solidity selector at memory offset 0. The 4-byte selector occupies the
//! high-order bytes (memory addresses 0..4) and the remaining 28 bytes are
//! zero. The hex strings are consumed verbatim by 256-bit integer constructors
//! such as `Context::word_const_str_hex`, so the selector ends up in the most
//! significant bits of the resulting `i256`.

/// 32-byte EVM word holding the Solidity `Panic(uint256)` ABI selector.
///
/// `keccak256("Panic(uint256)")[..4] == 0x4e487b71`, placed in the high-order
/// bytes of a 32-byte word followed by 28 zero bytes.
pub const PANIC_UINT256_SELECTOR_WORD_HEX: &str =
    "4e487b7100000000000000000000000000000000000000000000000000000000";

/// 32-byte EVM word holding the Solidity `Error(string)` ABI selector.
///
/// `keccak256("Error(string)")[..4] == 0x08c379a0`, placed in the high-order
/// bytes of a 32-byte word followed by 28 zero bytes.
pub const ERROR_STRING_SELECTOR_WORD_HEX: &str =
    "08c379a000000000000000000000000000000000000000000000000000000000";
