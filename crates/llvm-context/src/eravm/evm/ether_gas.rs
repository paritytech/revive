//!
//! Translates the value and balance operations.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `gas` instruction.
///
pub fn gas<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context.integer_const(256, 0).as_basic_value_enum())
}

///
/// Translates the `value` instruction.
///
pub fn value<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context.integer_const(256, 0).as_basic_value_enum())
}

///
/// Translates the `balance` instructions.
///
pub fn balance<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
