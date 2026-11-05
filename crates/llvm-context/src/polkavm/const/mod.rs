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

/// The library address linker symbol name prefix.
pub static GLOBAL_LIBRARY_ADDRESS_PREFIX: &str = "__library_address_";

/// The metadata hash global variable name.
pub static GLOBAL_METADATA_HASH: &str = "__revive_metadata_hash";

/// The ELF section carrying the metadata hash from the contract object to the PVM linker.
///
/// It is excluded from the final ELF and never loaded at runtime. The PVM linker
/// reads it from the object and emits it as the blob's metadata hash section.
pub static METADATA_HASH_SECTION: &str = ".revive_metadata_hash";

/// The LLVM metadata kind excluding a global's section from the final ELF.
pub static LLVM_METADATA_KIND_EXCLUDE: &str = "exclude";

/// The deployer call header size that consists of:
/// - bytecode hash (32 bytes)
pub const DEPLOYER_CALL_HEADER_SIZE: usize = BYTE_LENGTH_WORD;
