//! Runtime API import and export symbols.

/// The contract deploy export.
pub static CALL: &str = "call";

/// The contract call export.
pub static DEPLOY: &str = "deploy";

/// All exported symbols.
/// Useful for configuring common attributes and linkage.
pub static EXPORTS: [&str; 2] = [CALL, DEPLOY];

pub static ADDRESS: &str = "address";

pub static BLOCK_NUMBER: &str = "block_number";

pub static CALLER: &str = "caller";

pub static GET_STORAGE: &str = "get_storage";

pub static HASH_KECCAK_256: &str = "hash_keccak_256";

pub static INPUT: &str = "input";

pub static NOW: &str = "now";

pub static RETURN: &str = "seal_return";

pub static SET_STORAGE: &str = "set_storage";

pub static VALUE_TRANSFERRED: &str = "value_transferred";

/// All imported runtime API symbols..
/// Useful for configuring common attributes and linkage.
pub static IMPORTS: [&str; 6] = [
    GET_STORAGE,
    HASH_KECCAK_256,
    INPUT,
    RETURN,
    SET_STORAGE,
    VALUE_TRANSFERRED,
];

/// PolkaVM __sbrk API symbol to extend the heap memory.
pub static SBRK: &str = "__sbrk";
