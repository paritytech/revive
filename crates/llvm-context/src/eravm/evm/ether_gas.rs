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
    context: &mut Context<'ctx, D>,
    address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_ETH_TOKEN.into()),
        "balanceOf(uint256)",
        vec![address],
    )
}
