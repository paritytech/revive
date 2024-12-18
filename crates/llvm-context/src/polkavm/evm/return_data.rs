//! Translates the return data instructions.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the return data size.
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let return_data_size_value = context
        .build_runtime_call(revive_runtime_api::polkavm_imports::RETURNDATASIZE, &[])
        .expect("the return_data_size syscall method should return a value")
        .into_int_value();

    Ok(context
        .builder()
        .build_int_z_extend(
            return_data_size_value,
            context.word_type(),
            "return_data_size",
        )?
        .into())
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

    let output_pointer = context.builder().build_ptr_to_int(
        context.build_heap_gep(destination_offset, size)?.value,
        context.xlen_type(),
        "return_data_copy_output_pointer",
    )?;

    let output_length_pointer = context.build_alloca_at_entry(
        context.xlen_type(),
        "return_data_copy_output_length_pointer",
    );
    context.build_store(output_length_pointer, size)?;
    let output_length_pointer_int = context.builder().build_ptr_to_int(
        output_length_pointer.value,
        context.xlen_type(),
        "return_data_copy_output_length_pointer_int",
    )?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::RETURNDATACOPY,
        &[
            output_pointer.into(),
            output_length_pointer_int.into(),
            source_offset.into(),
        ],
    );

    // Trap on OOB (will be different in EOF code)
    let overflow_block = context.append_basic_block("return_data_overflow");
    let non_overflow_block = context.append_basic_block("return_data_non_overflow");
    let is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        size,
        context
            .build_load(output_length_pointer, "bytes_written")?
            .into_int_value(),
        "is_overflow",
    )?;
    context.build_conditional_branch(is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    context.build_call(context.intrinsics().trap, &[], "invalid_trap");
    context.build_unreachable();

    context.set_basic_block(non_overflow_block);
    Ok(())
}
