//! Translates the cryptographic operations.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

/// Translates the `sha3` instruction.
pub fn sha3<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let offset_casted = context.safe_truncate_int_to_xlen(offset)?;
    let length_casted = context.safe_truncate_int_to_xlen(length)?;
    let input_pointer = context.build_heap_gep(offset_casted, length_casted)?;
    let output_pointer = context.build_alloca(context.word_type(), "output_pointer");

    context.build_runtime_call(
        runtime_api::imports::HASH_KECCAK_256,
        &[
            input_pointer.to_int(context).into(),
            length_casted.into(),
            output_pointer.to_int(context).into(),
        ],
    );

    context.build_byte_swap(context.build_load(output_pointer, "sha3_output")?)
}
