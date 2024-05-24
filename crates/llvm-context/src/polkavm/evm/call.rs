//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates a contract call.
/// If the `simulation_address` is specified, the call is substituted with another instruction
/// according to the specification.
pub fn default<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _gas: inkwell::values::IntValue<'ctx>,
    _address: inkwell::values::IntValue<'ctx>,
    _value: Option<inkwell::values::IntValue<'ctx>>,
    _input_offset: inkwell::values::IntValue<'ctx>,
    _input_length: inkwell::values::IntValue<'ctx>,
    _output_offset: inkwell::values::IntValue<'ctx>,
    _output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!();
}

/// Translates the Yul `linkersymbol` instruction.
pub fn linker_symbol<'ctx, D>(
    context: &mut Context<'ctx, D>,
    mut arguments: [Argument<'ctx>; 1],
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let path = arguments[0]
        .original
        .take()
        .ok_or_else(|| anyhow::anyhow!("Linker symbol literal is missing"))?;

    Ok(context
        .resolve_library(path.as_str())?
        .as_basic_value_enum())
}
