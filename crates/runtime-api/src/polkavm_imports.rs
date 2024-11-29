//! This crate vendors the [PolkaVM][0] C API and provides a LLVM module for interacting
//! with the `pallet-revive` runtime API.
//! At present, the revive pallet requires blobs to export `call` and `deploy`,
//! and offers a bunch of [runtime API methods][1]. The provided [module] implements
//! those exports and imports.
//! [0]: [https://crates.io/crates/polkavm]
//! [1]: [https://docs.rs/pallet-contracts/26.0.0/pallet_contracts/api_doc/index.html]

use inkwell::{context::Context, memory_buffer::MemoryBuffer, module::Module, support::LLVMString};

include!(concat!(env!("OUT_DIR"), "/polkavm_imports.rs"));

pub static SBRK: &str = "__sbrk_internal";

pub static MEMORY_SIZE: &str = "__msize";

pub static ADDRESS: &str = "address";

pub static BALANCE: &str = "balance";

pub static BALANCE_OF: &str = "balance_of";

pub static BLOCK_HASH: &str = "block_hash";

pub static BLOCK_NUMBER: &str = "block_number";

pub static CALL: &str = "call";

pub static DELEGATE_CALL: &str = "delegate_call";

pub static CALLER: &str = "caller";

pub static CHAIN_ID: &str = "chain_id";

pub static CODE_SIZE: &str = "code_size";

pub static CODE_HASH: &str = "code_hash";

pub static DEPOSIT_EVENT: &str = "deposit_event";

pub static GET_IMMUTABLE_DATA: &str = "get_immutable_data";

pub static GET_STORAGE: &str = "get_storage";

pub static HASH_KECCAK_256: &str = "hash_keccak_256";

pub static INPUT: &str = "input";

pub static INSTANTIATE: &str = "instantiate";

pub static NOW: &str = "now";

pub static ORIGIN: &str = "origin";

pub static RETURN: &str = "seal_return";

pub static SET_STORAGE: &str = "set_storage";

pub static RETURNDATACOPY: &str = "return_data_copy";

pub static RETURNDATASIZE: &str = "return_data_size";

pub static SET_IMMUTABLE_DATA: &str = "set_immutable_data";

pub static VALUE_TRANSFERRED: &str = "value_transferred";

pub static WEIGHT_TO_FEE: &str = "weight_to_fee";

/// All imported runtime API symbols.
/// Useful for configuring common attributes and linkage.
pub static IMPORTS: [&str; 28] = [
    SBRK,
    MEMORY_SIZE,
    ADDRESS,
    BALANCE,
    BALANCE_OF,
    BLOCK_HASH,
    BLOCK_NUMBER,
    CALL,
    DELEGATE_CALL,
    CALLER,
    CHAIN_ID,
    CODE_SIZE,
    CODE_HASH,
    DEPOSIT_EVENT,
    GET_IMMUTABLE_DATA,
    GET_STORAGE,
    HASH_KECCAK_256,
    INPUT,
    INSTANTIATE,
    NOW,
    ORIGIN,
    RETURN,
    RETURNDATACOPY,
    RETURNDATASIZE,
    SET_IMMUTABLE_DATA,
    SET_STORAGE,
    VALUE_TRANSFERRED,
    WEIGHT_TO_FEE,
];

/// Creates a LLVM module from the [BITCODE].
/// The module imports `pallet-revive` runtime API functions.
/// Returns `Error` if the bitcode fails to parse, which should never happen.
pub fn module<'context>(
    context: &'context Context,
    module_name: &str,
) -> Result<Module<'context>, LLVMString> {
    let buf = MemoryBuffer::create_from_memory_range(BITCODE, module_name);
    Module::parse_bitcode_from_buffer(&buf, context)
}

#[cfg(test)]
mod tests {
    use crate::polkavm_imports;

    #[test]
    fn it_works() {
        inkwell::targets::Target::initialize_riscv(&Default::default());
        let context = inkwell::context::Context::create();
        let _ = polkavm_imports::module(&context, "polkavm_imports").unwrap();
    }
}
