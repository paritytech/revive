//! Custom RISC-V extension in PolkaVM that are partially supported.
//! We use inline assembly to emit partially supported instructions.

use inkwell::{context::Context, module::Module, support::LLVMString};

include!(concat!(env!("OUT_DIR"), "/bswap.rs"));

/// Returns a LLVM module containing a `__bswap` function, which
/// - Takes a `i256` value argument
/// - Byte swaps it using `rev8` from the `zbb` extension
/// - Returns the `i256` value
/// Returns `Error` if the module fails to validate, which should never happen.
pub fn module<'context>(
    context: &'context Context,
    module_name: &str,
) -> Result<Module<'context>, LLVMString> {
    let module = context.create_module(module_name);

    module.set_inline_assembly(ASSEMBLY);
    module.verify()?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    #[test]
    fn assembly_contains_rev8_instruction() {
        assert!(crate::ASSEMBLY.contains("rev8"));
    }

    #[test]
    fn module_is_valid() {
        inkwell::targets::Target::initialize_riscv(&Default::default());
        let context = inkwell::context::Context::create();

        assert!(crate::module(&context, "polkavm_bswap").is_ok());
    }
}
