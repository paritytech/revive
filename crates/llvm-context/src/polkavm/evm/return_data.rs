//! Translates the return data instructions.

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the return data size.
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    match context.get_global_value(crate::polkavm::GLOBAL_RETURN_DATA_SIZE) {
        Ok(global) => Ok(global),
        Err(_error) => Ok(context.word_const(0).as_basic_value_enum()),
    }
}

/// Translates the return data copy.
pub fn copy<'ctx, D>(
    context: &mut Context<'ctx, D>,
    destination_offset: inkwell::values::IntValue<'ctx>,
    source_offset: inkwell::values::IntValue<'ctx>,
    size: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let entry_block = context.append_basic_block("return_data_copy_entry_block");
    let error_block = context.append_basic_block("return_data_copy_error_block");
    let join_block = context.append_basic_block("return_data_copy_join_block");

    context.set_basic_block(entry_block);

    let return_data_size = self::size(context)?.into_int_value();
    let copy_slice_end =
        context
            .builder()
            .build_int_add(source_offset, size, "return_data_copy_slice_end")?;

    let is_copy_out_of_bounds = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        copy_slice_end,
        return_data_size,
        "return_data_copy_is_out_of_bounds",
    )?;
    context.build_conditional_branch(is_copy_out_of_bounds, error_block, join_block)?;

    context.set_basic_block(error_block);
    crate::polkavm::evm::r#return::revert(context, context.word_const(0), context.word_const(0))?;

    context.set_basic_block(join_block);
    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        destination_offset,
        "return_data_copy_destination_pointer",
    );

    let return_data_pointer_global =
        context.get_global(crate::polkavm::GLOBAL_RETURN_DATA_POINTER)?;
    let return_data_pointer_pointer = return_data_pointer_global.into();
    let return_data_pointer =
        context.build_load(return_data_pointer_pointer, "return_data_pointer")?;
    let source = context.build_gep(
        Pointer::new(
            context.byte_type(),
            return_data_pointer_pointer.address_space,
            return_data_pointer.into_pointer_value(),
        ),
        &[source_offset],
        context.byte_type().as_basic_type_enum(),
        "return_data_source_pointer",
    );

    context.build_memcpy(
        context.intrinsics().memory_copy_from_generic,
        destination,
        source,
        size,
        "return_data_copy_memcpy_from_return_data",
    )?;

    let heap_pointer = context.get_global(crate::polkavm::GLOBAL_HEAP_MEMORY_POINTER)?;
    let heap_pointer_pointer = heap_pointer.into();
    let heap_pointer_value =
        context.build_load(heap_pointer_pointer, "return_data_copy_heap_pointer")?;

    let heap_pointer_int = context.builder().build_ptr_to_int(
        heap_pointer_value.into_pointer_value(),
        context.word_type(),
        "heap_pointer_int",
    )?;

    let new_heap_pointer = context.builder().build_int_add(
        heap_pointer_int,
        size,
        "return_data_copy_new_heap_pointer",
    )?;

    let new_heap_pointer_ptr = context.builder().build_int_to_ptr(
        new_heap_pointer,
        heap_pointer_pointer.value.get_type(),
        "new_heap_pointer_ptr",
    )?;

    context.build_store(heap_pointer_pointer, new_heap_pointer_ptr)?;

    Ok(())
}
