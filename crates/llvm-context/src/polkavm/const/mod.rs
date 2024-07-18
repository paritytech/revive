//! The LLVM context constants.

/// Runtime API methods.
pub mod runtime_api;

/// The LLVM framework version.
pub const LLVM_VERSION: semver::Version = semver::Version::new(18, 1, 4);

/// The PolkaVM version.
pub const ZKEVM_VERSION: semver::Version = semver::Version::new(1, 3, 2);

/// The register width sized type
pub static XLEN: usize = revive_common::BIT_LENGTH_X32;

/// The heap memory pointer pointer global variable name.
pub static GLOBAL_HEAP_MEMORY_POINTER: &str = "memory_pointer";

/// The calldata pointer global variable name.
pub static GLOBAL_CALLDATA_POINTER: &str = "ptr_calldata";

/// The calldata size global variable name.
pub static GLOBAL_CALLDATA_SIZE: &str = "calldatasize";

/// The return data pointer global variable name.
pub static GLOBAL_RETURN_DATA_POINTER: &str = "ptr_return_data";

/// The return data size pointer global variable name.
pub static GLOBAL_RETURN_DATA_SIZE: &str = "returndatasize";

/// The call flags global variable name.
pub static GLOBAL_CALL_FLAGS: &str = "call_flags";

/// The extra ABI data global variable name.
pub static GLOBAL_EXTRA_ABI_DATA: &str = "extra_abi_data";

/// The constant array global variable name prefix.
pub static GLOBAL_CONST_ARRAY_PREFIX: &str = "const_array_";

/// The global verbatim getter identifier prefix.
pub static GLOBAL_VERBATIM_GETTER_PREFIX: &str = "get_global::";

/// The static word size.
pub static GLOBAL_I256_SIZE: &str = "i256_size";

/// The static value size.
pub static GLOBAL_I160_SIZE: &str = "i160_size";

/// The static i64 size.
pub static GLOBAL_I64_SIZE: &str = "i64_size";

/// The external call data offset in the auxiliary heap.
pub const HEAP_AUX_OFFSET_EXTERNAL_CALL: u64 = 0;

/// The constructor return data offset in the auxiliary heap.
pub const HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA: u64 =
    8 * (revive_common::BYTE_LENGTH_WORD as u64);

/// The number of the extra ABI data arguments.
pub const EXTRA_ABI_DATA_SIZE: usize = 0;

/// The `create` method deployer signature.
pub static DEPLOYER_SIGNATURE_CREATE: &str = "create(bytes32,bytes32,bytes)";

/// The `create2` method deployer signature.
pub static DEPLOYER_SIGNATURE_CREATE2: &str = "create2(bytes32,bytes32,bytes)";

/// The absence of system call bit.
pub const NO_SYSTEM_CALL_BIT: bool = false;

/// The system call bit.
pub const SYSTEM_CALL_BIT: bool = true;

/// The deployer call header size that consists of:
/// - bytecode hash (32 bytes)
pub const DEPLOYER_CALL_HEADER_SIZE: usize = revive_common::BYTE_LENGTH_WORD;
