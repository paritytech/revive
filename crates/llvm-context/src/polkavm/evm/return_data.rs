//! Translates the return data instructions.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the return data size.
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let value = context
        .get_global_value(crate::polkavm::GLOBAL_RETURN_DATA_SIZE)?
        .into_int_value();
    Ok(context
        .builder()
        .build_int_z_extend(value, context.word_type(), "calldatasize_extended")?
        .as_basic_value_enum())
}

/// Translates the return data copy, trapping if
/// - Destination, offset or size exceed the VM register size (XLEN)
/// - `source_offset + size` overflows (in XLEN)
/// - `source_offset + size` is beyond `RETURNDATASIZE`
pub fn copy<'ctx, D>(
    context: &mut Context<'ctx, D>,
    destination_offset: inkwell::values::IntValue<'ctx>,
    source_offset: inkwell::values::IntValue<'ctx>,
    size: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let source_offset = context.safe_truncate_int_to_xlen(source_offset)?;
    let destination_offset = context.safe_truncate_int_to_xlen(destination_offset)?;
    let size = context.safe_truncate_int_to_xlen(size)?;

    let block_copy = context.append_basic_block("copy_block");
    let block_trap = context.append_basic_block("trap_block");
    let block_check_out_of_bounds = context.append_basic_block("check_out_of_bounds_block");
    let is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        source_offset,
        context.builder().build_int_sub(
            context.xlen_type().const_all_ones(),
            size,
            "offset_plus_size_max_value",
        )?,
        "is_returndata_size_out_of_bounds",
    )?;
    context.build_conditional_branch(is_overflow, block_trap, block_check_out_of_bounds)?;

    context.set_basic_block(block_check_out_of_bounds);
    let is_out_of_bounds = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        context.builder().build_int_add(
            source_offset,
            context
                .get_global_value(crate::polkavm::GLOBAL_RETURN_DATA_SIZE)?
                .into_int_value(),
            "returndata_end_pointer",
        )?,
        context
            .xlen_type()
            .const_int(crate::PolkaVMEntryFunction::MAX_CALLDATA_SIZE as u64, false),
        "is_return_data_copy_overflow",
    )?;
    context.build_conditional_branch(is_out_of_bounds, block_trap, block_copy)?;

    context.set_basic_block(block_trap);
    context.build_call(context.intrinsics().trap, &[], "invalid_returndata_copy");
    context.build_unreachable();

    context.set_basic_block(block_copy);
    context.build_memcpy(
        context.build_heap_gep(destination_offset, size)?,
        context.build_gep(
            context
                .get_global(crate::polkavm::GLOBAL_RETURN_DATA_POINTER)?
                .into(),
            &[context.xlen_type().const_zero(), source_offset],
            context.byte_type(),
            "source_offset_gep",
        ),
        size,
        "return_data_copy_memcpy_from_return_data",
    )
}
