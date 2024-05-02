//! Translates the context getter instructions.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `gas_limit` instruction.
pub fn gas_limit<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `gas_price` instruction.
pub fn gas_price<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `tx.origin` instruction.
pub fn origin<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `chain_id` instruction.
pub fn chain_id<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `block_number` instruction.
pub fn block_number<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `block_timestamp` instruction.
pub fn block_timestamp<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `block_hash` instruction.
pub fn block_hash<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `difficulty` instruction.
pub fn difficulty<'ctx, D>(
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
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
    _context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!()
}

/// Translates the `msize` instruction.
pub fn msize<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let heap_end = context.build_sbrk(context.xlen_type().const_zero())?;
    let heap_start = context
        .get_global(crate::polkavm::GLOBAL_HEAP_MEMORY_POINTER)?
        .value
        .as_pointer_value();
    let heap_size = context.builder().build_int_nuw_sub(
        context
            .builder()
            .build_ptr_to_int(heap_end, context.xlen_type(), "heap_end")?,
        context
            .builder()
            .build_ptr_to_int(heap_start, context.xlen_type(), "heap_start")?,
        "heap_size",
    )?;
    Ok(context
        .builder()
        .build_int_z_extend(heap_size, context.field_type(), "heap_size_extended")?
        .as_basic_value_enum())
}
