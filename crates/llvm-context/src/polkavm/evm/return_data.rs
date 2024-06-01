//! Translates the return data instructions.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

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

    let destination_offset = context.builder().build_ptr_to_int(
        context.build_heap_gep(destination_offset, size)?.value,
        context.xlen_type(),
        "destination_offset",
    )?;

    context.build_runtime_call(
        runtime_api::imports::RETURNDATACOPY,
        &[destination_offset.into(), source_offset.into(), size.into()],
    );

    Ok(())
}
