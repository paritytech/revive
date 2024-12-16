//! Translates the calldata instructions.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the calldata load.
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let output_pointer = context.build_alloca_at_entry(context.word_type(), "call_data_output");
    let offset = context.safe_truncate_int_to_xlen(offset)?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::CALL_DATA_LOAD,
        &[output_pointer.to_int(context).into(), offset.into()],
    );

    context.build_load(output_pointer, "call_data_load_value")
}

/// Translates the calldata size.
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let value = context.get_global_value(crate::polkavm::GLOBAL_CALLDATA_SIZE)?;
    Ok(context
        .builder()
        .build_int_z_extend(
            value.into_int_value(),
            context.word_type(),
            "call_data_size_value",
        )?
        .into())
}

/// Translates the calldata copy.
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
    let size = context.safe_truncate_int_to_xlen(size)?;
    let destination_offset = context.safe_truncate_int_to_xlen(destination_offset)?;
    let output_pointer = context.build_heap_gep(destination_offset, size)?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::CALL_DATA_COPY,
        &[
            output_pointer.to_int(context).into(),
            size.into(),
            source_offset.into(),
        ],
    );

    Ok(())
}
