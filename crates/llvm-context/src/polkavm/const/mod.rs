//! The LLVM context constants.

use revive_common::{BIT_LENGTH_X32, BYTE_LENGTH_WORD};

/// The LLVM framework version.
pub const LLVM_VERSION: semver::Version = semver::Version::new(18, 1, 4);

/// The pointer width sized type.
pub static XLEN: usize = BIT_LENGTH_X32;

/// The calldata size global variable name.
pub static GLOBAL_CALLDATA_SIZE: &str = "calldatasize";

/// The heap size global variable name.
pub static GLOBAL_HEAP_SIZE: &str = "__heap_size";

/// The heap memory global variable name.
pub static GLOBAL_HEAP_MEMORY: &str = "__heap_memory";

/// The spill buffer global variable name.
pub static GLOBAL_ADDRESS_SPILL_BUFFER: &str = "address_spill_buffer";

/// The deployer call header size that consists of:
/// - bytecode hash (32 bytes)
pub const DEPLOYER_CALL_HEADER_SIZE: usize = BYTE_LENGTH_WORD;
