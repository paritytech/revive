//! Translates the value and balance operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `gas` instruction.
pub fn gas<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context.integer_const(256, 0).as_basic_value_enum())
}

/// Translates the `value` instruction.
pub fn value<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let output_pointer = context.build_alloca(context.value_type(), "output_pointer");
    let output_pointer_casted = context.builder().build_ptr_to_int(
        output_pointer.value,
        context.xlen_type(),
        "output_pointer_casted",
    )?;

    let output_length_pointer = context.build_alloca(context.xlen_type(), "output_len_pointer");
    let output_length_pointer_casted = context.builder().build_ptr_to_int(
        output_length_pointer.value,
        context.xlen_type(),
        "output_pointer_casted",
    )?;
    context.build_store(
        output_length_pointer,
        context.integer_const(
            crate::polkavm::XLEN,
            revive_common::BYTE_LENGTH_VALUE as u64,
        ),
    )?;

    context.builder().build_call(
        context
            .module()
            .get_function("value_transferred")
            .expect("is declared"),
        &[
            output_pointer_casted.into(),
            output_length_pointer_casted.into(),
        ],
        "call_seal_value_transferred",
    )?;

    let value = context.build_load(output_pointer, "transferred_value")?;
    let value_extended = context.builder().build_int_z_extend(
        value.into_int_value(),
        context.field_type(),
        "transferred_value_extended",
    )?;
    Ok(value_extended.as_basic_value_enum())
}

/// Translates the `balance` instructions.
pub fn balance<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
