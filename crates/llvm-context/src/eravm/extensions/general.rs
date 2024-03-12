//!
//! Translates the general instructions of the EraVM Yul extension.
//!

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Generates a call to L1.
///
pub fn to_l1<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _is_first: inkwell::values::IntValue<'ctx>,
    _in_0: inkwell::values::IntValue<'ctx>,
    _in_1: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Generates a `code source` call.
///
pub fn code_source<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

///
/// Generates a precompile call.
///
pub fn precompile<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _in_0: inkwell::values::IntValue<'ctx>,
    _gas_left: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

///
/// Generates a `meta` call.
///
pub fn meta<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Generates a `u128` context value setter call.
///
pub fn set_context_value<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Generates a public data price setter call.
///
pub fn set_pubdata_price<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Generates a transaction counter increment call.
///
pub fn increment_tx_counter<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Generates an event call.
///
pub fn event<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _operand_1: inkwell::values::IntValue<'ctx>,
    _operand_2: inkwell::values::IntValue<'ctx>,
    _is_initializer: bool,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
