//!
//! Translates the context getter instructions.
//!

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `gas_limit` instruction.
///
pub fn gas_limit<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "blockGasLimit()",
        vec![],
    )
}

///
/// Translates the `gas_price` instruction.
///
pub fn gas_price<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "gasPrice()",
        vec![],
    )
}

///
/// Translates the `tx.origin` instruction.
///
pub fn origin<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "origin()",
        vec![],
    )
}

///
/// Translates the `chain_id` instruction.
///
pub fn chain_id<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "chainId()",
        vec![],
    )
}

///
/// Translates the `block_number` instruction.
///
pub fn block_number<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "getBlockNumber()",
        vec![],
    )
}

///
/// Translates the `block_timestamp` instruction.
///
pub fn block_timestamp<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "getBlockTimestamp()",
        vec![],
    )
}

///
/// Translates the `block_hash` instruction.
///
pub fn block_hash<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "getBlockHashEVM(uint256)",
        vec![index],
    )
}

///
/// Translates the `difficulty` instruction.
///
pub fn difficulty<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "difficulty()",
        vec![],
    )
}

///
/// Translates the `coinbase` instruction.
///
pub fn coinbase<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "coinbase()",
        vec![],
    )
}

///
/// Translates the `basefee` instruction.
///
pub fn basefee<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::call::request(
        context,
        context.field_const(zkevm_opcode_defs::ADDRESS_SYSTEM_CONTEXT.into()),
        "baseFee()",
        vec![],
    )
}

///
/// Translates the `msize` instruction.
///
pub fn msize<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}
