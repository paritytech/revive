//! Translates the value and balance operations.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `gas` instruction.
pub fn gas<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let ref_time_left_value = context
        .build_runtime_call(revive_runtime_api::polkavm_imports::REF_TIME_LEFT, &[])
        .expect("the ref_time_left syscall method should return a value")
        .into_int_value();

    Ok(context
        .builder()
        .build_int_z_extend(ref_time_left_value, context.word_type(), "gas_left")?
        .into())
}

/// Translates the `value` instruction.
pub fn value<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let output_pointer = context.build_alloca_at_entry(context.value_type(), "value_transferred");
    context.build_store(output_pointer, context.word_const(0))?;
    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::VALUE_TRANSFERRED,
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
    let address_pointer = context.build_address_argument_store(address)?;
    let balance_pointer = context.build_alloca_at_entry(context.word_type(), "balance_pointer");
    let balance = context.builder().build_ptr_to_int(
        balance_pointer.value,
        context.xlen_type(),
        "balance",
    )?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::BALANCE_OF,
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
    let balance_pointer = context.build_alloca_at_entry(context.word_type(), "balance_pointer");
    let balance = context.builder().build_ptr_to_int(
        balance_pointer.value,
        context.xlen_type(),
        "balance",
    )?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::BALANCE,
        &[balance.into()],
    );

    context.build_load(balance_pointer, "balance")
}
