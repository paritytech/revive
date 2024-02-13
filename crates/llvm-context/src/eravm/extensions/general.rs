//!
//! Translates the general instructions of the EraVM Yul extension.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Generates a call to L1.
///
pub fn to_l1<'ctx, D>(
    context: &mut Context<'ctx, D>,
    is_first: inkwell::values::IntValue<'ctx>,
    in_0: inkwell::values::IntValue<'ctx>,
    in_1: inkwell::values::IntValue<'ctx>,
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
    context: &mut Context<'ctx, D>,
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
    context: &mut Context<'ctx, D>,
    in_0: inkwell::values::IntValue<'ctx>,
    gas_left: inkwell::values::IntValue<'ctx>,
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
    context: &mut Context<'ctx, D>,
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
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
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
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
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
    context: &mut Context<'ctx, D>,
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
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    is_initializer: bool,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
