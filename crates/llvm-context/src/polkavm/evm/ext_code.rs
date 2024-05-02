//! Translates the external code operations.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `extcodesize` instruction.
pub fn size<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `extcodehash` instruction.
pub fn hash<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
