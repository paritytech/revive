//! The LLVM context constants.

/// The LLVM framework version.
pub const LLVM_VERSION: semver::Version = semver::Version::new(18, 1, 4);

/// The pointer width sized type.
pub static XLEN: usize = revive_common::BIT_LENGTH_X32;

/// The calldata size global variable name.
pub static GLOBAL_CALLDATA_SIZE: &str = "calldatasize";

/// The spill buffer global variable name.
pub static GLOBAL_ADDRESS_SPILL_BUFFER: &str = "address_spill_buffer";

/// The deployer call header size that consists of:
/// - bytecode hash (32 bytes)
pub const DEPLOYER_CALL_HEADER_SIZE: usize = revive_common::BYTE_LENGTH_WORD;
