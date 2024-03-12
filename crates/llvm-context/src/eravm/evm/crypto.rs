//!
//! Translates the cryptographic operations.
//!

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `sha3` instruction.
///
pub fn sha3<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _offset: inkwell::values::IntValue<'ctx>,
    _length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
