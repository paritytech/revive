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
    let output_pointer = context.build_alloca(context.value_type(), "value_transferred");
    context.build_store(output_pointer, context.word_const(0))?;
    context.build_runtime_call(
        runtime_api::imports::VALUE_TRANSFERRED,
        &[output_pointer.to_int(context).into()],
    );
    context.build_load(output_pointer, "value_transferred")
}

/// Translates the `balance` instructions.
pub fn balance<'ctx, D>(
    context: &mut Context<'ctx, D>,
    address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address_type = context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
    let address_pointer = context.build_alloca_at_entry(address_type, "address_pointer");
    let address_truncated =
        context
            .builder()
            .build_int_truncate(address, address_type, "address_truncated")?;
    let address_swapped = context.build_byte_swap(address_truncated.into())?;
    context.build_store(address_pointer, address_swapped)?;

    let balance_pointer = context.build_alloca(context.word_type(), "balance_pointer");
    let balance = context.builder().build_ptr_to_int(
        balance_pointer.value,
        context.xlen_type(),
        "balance",
    )?;

    context.build_runtime_call(
        runtime_api::imports::BALANCE_OF,
        &[address_pointer.to_int(context).into(), balance.into()],
    );

    context.build_load(balance_pointer, "balance")
}

/// Translates the `selfbalance` instructions.
pub fn self_balance<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let balance_pointer = context.build_alloca(context.word_type(), "balance_pointer");
    let balance = context.builder().build_ptr_to_int(
        balance_pointer.value,
        context.xlen_type(),
        "balance",
    )?;

    context.build_runtime_call(runtime_api::imports::BALANCE, &[balance.into()]);

    context.build_load(balance_pointer, "balance")
}
