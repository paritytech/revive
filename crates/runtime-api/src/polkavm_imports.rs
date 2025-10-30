use inkwell::{context::Context, memory_buffer::MemoryBuffer, module::Module, support::LLVMString};

include!(concat!(env!("OUT_DIR"), "/polkavm_imports.rs"));

pub static ADDRESS: &str = "address";

pub static BALANCE: &str = "balance";

pub static BALANCE_OF: &str = "balance_of";

pub static BASE_FEE: &str = "base_fee";

pub static BLOCK_AUTHOR: &str = "block_author";

pub static BLOCK_HASH: &str = "block_hash";

pub static BLOCK_NUMBER: &str = "block_number";

pub static CALL: &str = "call";

pub static CALL_DATA_COPY: &str = "call_data_copy";

pub static CALL_DATA_LOAD: &str = "call_data_load";

pub static CALL_DATA_SIZE: &str = "call_data_size";

pub static CALLER: &str = "caller";

pub static CHAIN_ID: &str = "chain_id";

pub static CODE_SIZE: &str = "code_size";

pub static CODE_HASH: &str = "code_hash";

pub static DELEGATE_CALL: &str = "delegate_call";

pub static DEPOSIT_EVENT: &str = "deposit_event";

pub static GAS_LIMIT: &str = "gas_limit";

pub static GAS_PRICE: &str = "gas_price";

pub static GET_IMMUTABLE_DATA: &str = "get_immutable_data";

pub static GET_STORAGE: &str = "get_storage_or_zero";

pub static HASH_KECCAK_256: &str = "hash_keccak_256";

pub static INSTANTIATE: &str = "instantiate";

pub static NOW: &str = "now";

pub static ORIGIN: &str = "origin";

pub static REF_TIME_LEFT: &str = "ref_time_left";

pub static RETURN: &str = "seal_return";

pub static RETURNDATACOPY: &str = "return_data_copy";

pub static RETURNDATASIZE: &str = "return_data_size";

pub static SET_IMMUTABLE_DATA: &str = "set_immutable_data";

pub static SET_STORAGE: &str = "set_storage_or_clear";

pub static VALUE_TRANSFERRED: &str = "value_transferred";

pub static WEIGHT_TO_FEE: &str = "weight_to_fee";

/// All imported runtime API symbols.
/// Useful for configuring common attributes and linkage.
pub static IMPORTS: [&str; 33] = [
    ADDRESS,
    BALANCE,
    BALANCE_OF,
    BASE_FEE,
    BLOCK_AUTHOR,
    BLOCK_HASH,
    BLOCK_NUMBER,
    CALL,
    CALL_DATA_COPY,
    CALL_DATA_LOAD,
    CALL_DATA_SIZE,
    CALLER,
    CHAIN_ID,
    CODE_SIZE,
    CODE_HASH,
    DELEGATE_CALL,
    DEPOSIT_EVENT,
    GAS_LIMIT,
    GAS_PRICE,
    GET_IMMUTABLE_DATA,
    GET_STORAGE,
    HASH_KECCAK_256,
    INSTANTIATE,
    NOW,
    ORIGIN,
    REF_TIME_LEFT,
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
