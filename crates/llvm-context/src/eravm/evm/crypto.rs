//!
//! Translates the cryptographic operations.
//!

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `sha3` instruction.
///
pub fn sha3<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let offset_casted = context.safe_truncate_int_to_i32(offset)?;
    let length_casted = context.safe_truncate_int_to_i32(length)?;
    let input_pointer = context.build_heap_gep(offset_casted, length_casted)?;
    let input_pointer_casted = context.builder().build_ptr_to_int(
        input_pointer.value,
        context.integer_type(32),
        "input_pointer_casted",
    )?;

    let output_pointer = context.build_alloca(context.field_type(), "output_pointer");
    let output_pointer_casted = context.builder().build_ptr_to_int(
        output_pointer.value,
        context.integer_type(32),
        "output_pointer_casted",
    )?;

    let function = context
        .module()
        .get_function("hash_keccak_256")
        .expect("is declared");

    context.builder().build_call(
        function,
        &[
            input_pointer_casted.into(),
            length_casted.into(),
            output_pointer_casted.into(),
        ],
        "call_seal_hash_keccak_256",
    )?;

    Ok(context.build_byte_swap(context.build_load(output_pointer, "sha3_output")?))
}
