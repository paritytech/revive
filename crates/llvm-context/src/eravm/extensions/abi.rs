//!
//! Translates the ABI instructions of the EraVM Yul extension.
//!

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Generates an extra ABI data getter call.
///
pub fn get_extra_abi_data<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let extra_active_data_global = context.get_global(crate::eravm::GLOBAL_EXTRA_ABI_DATA)?;
    let extra_active_data_pointer = extra_active_data_global.into();
    let extra_active_data_element_pointer = context.build_gep(
        extra_active_data_pointer,
        &[context.field_const(0), index],
        context.field_type().as_basic_type_enum(),
        "extra_active_data_element_pointer",
    );
    context.build_load(
        extra_active_data_element_pointer,
        "extra_active_data_element_value",
    )
}

///
/// Loads the calldata pointer to the active pointer.
///
pub fn calldata_ptr_to_active<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let calldata_pointer = context.get_global_value(crate::eravm::GLOBAL_CALLDATA_POINTER)?;
    context.set_global(
        crate::eravm::GLOBAL_ACTIVE_POINTER,
        context.byte_type().ptr_type(AddressSpace::Generic.into()),
        AddressSpace::Stack,
        calldata_pointer,
    );
    Ok(context.field_const(1).as_basic_value_enum())
}

///
/// Loads the return data pointer to the active pointer.
///
pub fn return_data_ptr_to_active<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let calldata_pointer = context.get_global_value(crate::eravm::GLOBAL_RETURN_DATA_POINTER)?;
    context.set_global(
        crate::eravm::GLOBAL_ACTIVE_POINTER,
        context.byte_type().ptr_type(AddressSpace::Generic.into()),
        AddressSpace::Stack,
        calldata_pointer,
    );
    Ok(context.field_const(1).as_basic_value_enum())
}

///
/// Shifts the active pointer by the specified `offset`.
///
pub fn active_ptr_add_assign<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let active_pointer = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
    let active_pointer_shifted = context.build_gep(
        Pointer::new(
            context.byte_type(),
            AddressSpace::Generic,
            active_pointer.into_pointer_value(),
        ),
        &[offset],
        context.byte_type().as_basic_type_enum(),
        "active_pointer_shifted",
    );
    context.set_global(
        crate::eravm::GLOBAL_ACTIVE_POINTER,
        context.byte_type().ptr_type(AddressSpace::Generic.into()),
        AddressSpace::Stack,
        active_pointer_shifted.value,
    );
    Ok(context.field_const(1).as_basic_value_enum())
}

///
/// Shrinks the active pointer by the specified `offset`.
///
pub fn active_ptr_shrink_assign<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Writes the specified `data` into the upper 128 bits of the active pointer.
///
pub fn active_ptr_pack_assign<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _data: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    unimplemented!()
}

///
/// Loads a single word from the active pointer to the stack.
///
pub fn active_ptr_data_load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let active_pointer = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
    let active_pointer = context.build_gep(
        Pointer::new(
            context.byte_type(),
            AddressSpace::Generic,
            active_pointer.into_pointer_value(),
        ),
        &[offset],
        context.field_type().as_basic_type_enum(),
        "active_pointer_with_offset",
    );
    context.build_load(active_pointer, "active_pointer_value")
}

///
/// Returns the active pointer data size.
///
pub fn active_ptr_data_size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let active_pointer = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
    let active_pointer_value = context.builder().build_ptr_to_int(
        active_pointer.into_pointer_value(),
        context.field_type(),
        "active_pointer_value",
    )?;
    let active_pointer_value_shifted = context.builder().build_right_shift(
        active_pointer_value,
        context.field_const((era_compiler_common::BIT_LENGTH_X32 * 3) as u64),
        false,
        "active_pointer_value_shifted",
    )?;
    let active_pointer_length = context.builder().build_and(
        active_pointer_value_shifted,
        context.field_const(u32::MAX as u64),
        "active_pointer_length",
    )?;
    Ok(active_pointer_length.as_basic_value_enum())
}

///
/// Copies a chunk of data from the active pointer to the heap.
///
pub fn active_ptr_data_copy<'ctx, D>(
    context: &mut Context<'ctx, D>,
    destination_offset: inkwell::values::IntValue<'ctx>,
    source_offset: inkwell::values::IntValue<'ctx>,
    size: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        destination_offset,
        "active_pointer_data_copy_destination_pointer",
    );

    let active_pointer = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
    let source = context.build_gep(
        Pointer::new(
            context.byte_type(),
            AddressSpace::Generic,
            active_pointer.into_pointer_value(),
        ),
        &[source_offset],
        context.byte_type().as_basic_type_enum(),
        "active_pointer_data_copy_source_pointer",
    );

    context.build_memcpy(
        context.intrinsics().memory_copy_from_generic,
        destination,
        source,
        size,
        "active_pointer_data_copy_memcpy_from_child",
    )?;

    Ok(context.field_const(1).as_basic_value_enum())
}
