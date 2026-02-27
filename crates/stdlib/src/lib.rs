//! This crate vendors EVM related standard library functionality and provides a LLVM module,
//! exporting the standard library functions.
//! The standard library code is inherited and adapted from [the era compiler][0].
//!
//! [0]: [<https://github.com/matter-labs/era-compiler-llvm/blob/v1.4.1/llvm/lib/Target/PolkaVM/polkavm-stdlib.ll>]

use inkwell::{context::Context, memory_buffer::MemoryBuffer, module::Module, support::LLVMString};

include!(concat!(env!("OUT_DIR"), "/stdlib.rs"));

/// Creates a LLVM module from [BITCODE].
/// The module exports a bunch of EVM related "stdlib" functions.
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
        let module = crate::module(&context, "stdlib").unwrap();

        assert!(module.get_function("__signextend").is_some());
    }
}
