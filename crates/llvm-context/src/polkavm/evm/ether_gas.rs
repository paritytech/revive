//! Translates the value and balance operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

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
    let (output_pointer, output_length_pointer) =
        context.build_stack_parameter(revive_common::BIT_LENGTH_VALUE, "value_transferred_output");
    context.build_runtime_call(
        runtime_api::imports::VALUE_TRANSFERRED,
        &[
            output_pointer.to_int(context).into(),
            output_length_pointer.to_int(context).into(),
        ],
    );
    context.build_load_word(
        output_pointer,
        revive_common::BIT_LENGTH_VALUE,
        "value_transferred",
    )
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
