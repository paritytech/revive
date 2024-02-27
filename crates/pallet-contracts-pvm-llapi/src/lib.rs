//! This crate vendors the [PolkaVM][0] C API and provides a LLVM module for interacting
//! with the `pallet-contracts` runtime API.
//!
//! At present, the contracts pallet requires blobs to export `call` and `deploy`,
//! and offers a bunch of [runtime API methods][1]. The provided [module] implements
//! those exports and imports.
//!
//! [0]: [https://crates.io/crates/polkavm]
//! [1]: [https://docs.rs/pallet-contracts/26.0.0/pallet_contracts/api_doc/index.html]
//!

use inkwell::{context::Context, memory_buffer::MemoryBuffer, module::Module, support::LLVMString};

include!(concat!(env!("OUT_DIR"), "/polkavm_guest.rs"));

/// Creates a LLVM module from the [BITCODE].
///
/// The module does:
/// - Export the `call` and `deploy` functions (which are named thereafter).
/// - Import (most) `pallet-contracts` runtime API functions.
///
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
    #[test]
    fn it_works() {
        inkwell::targets::Target::initialize_riscv(&Default::default());
        let context = inkwell::context::Context::create();
        let module = crate::module(&context, "polkavm_guest").unwrap();

        assert!(module.get_function("call").is_some());
        assert!(module.get_function("deploy").is_some());
    }
}
