//! Translates the context getter instructions.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `gas_limit` instruction.
pub fn gas_limit<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let gas_limit_value = context
        .build_runtime_call(revive_runtime_api::polkavm_imports::GAS_LIMIT, &[])
        .expect("the gas_limit syscall method should return a value")
        .into_int_value();

    Ok(context
        .builder()
        .build_int_z_extend(gas_limit_value, context.word_type(), "gas_limit")?
        .into())
}

/// Translates the `gas_price` instruction.
pub fn gas_price<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let gas_price_value = context
        .build_runtime_call(revive_runtime_api::polkavm_imports::GAS_PRICE, &[])
        .expect("the gas_price syscall method should return a value")
        .into_int_value();

    Ok(context
        .builder()
        .build_int_z_extend(gas_price_value, context.word_type(), "gas_price")?
        .into())
}

/// Translates the `tx.origin` instruction.
pub fn origin<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address_type = context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
    let address_pointer = context.build_alloca_at_entry(address_type, "origin_address");
    context.build_store(address_pointer, address_type.const_zero())?;
    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::ORIGIN,
        &[address_pointer.to_int(context).into()],
    );
    context.build_load_address(address_pointer)
}

/// Translates the `chain_id` instruction.
pub fn chain_id<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    context.build_runtime_call_to_getter(revive_runtime_api::polkavm_imports::CHAIN_ID)
}

/// Translates the `block_number` instruction.
pub fn block_number<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    context.build_runtime_call_to_getter(revive_runtime_api::polkavm_imports::BLOCK_NUMBER)
}

/// Translates the `block_timestamp` instruction.
pub fn block_timestamp<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    context.build_runtime_call_to_getter(revive_runtime_api::polkavm_imports::NOW)
}

/// Translates the `block_hash` instruction.
pub fn block_hash<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let output_pointer = context.build_alloca_at_entry(context.word_type(), "blockhash_out_ptr");
    let index_ptr = context.build_alloca_at_entry(context.word_type(), "blockhash_index_ptr");
    context.build_store(index_ptr, index)?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::BLOCK_HASH,
        &[
            index_ptr.to_int(context).into(),
            output_pointer.to_int(context).into(),
        ],
    );
    context.build_byte_swap(context.build_load(output_pointer, "block_hash")?)
}

/// Translates the `difficulty` instruction.
pub fn difficulty<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context.word_const(2500000000000000).as_basic_value_enum())
}

/// Translates the `coinbase` instruction.
pub fn coinbase<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `basefee` instruction.
pub fn basefee<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context.word_const(0).as_basic_value_enum())
}

/// Translates the `address` instruction.
pub fn address<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let pointer = context.build_alloca_at_entry(
        context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS),
        "address_output",
    );
    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::ADDRESS,
        &[pointer.to_int(context).into()],
    );
    context.build_load_address(pointer)
}

/// Translates the `caller` instruction.
pub fn caller<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let pointer = context.build_alloca_at_entry(
        context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS),
        "address_output",
    );
    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::CALLER,
        &[pointer.to_int(context).into()],
    );
    context.build_load_address(pointer)
}
