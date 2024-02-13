//!
//! Translates the calldata instructions.
//!

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;
use inkwell::types::BasicType;

///
/// Translates the calldata load.
///
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let calldata_pointer_global = context.get_global(crate::eravm::GLOBAL_CALLDATA_POINTER)?;
    let calldata_pointer_pointer = calldata_pointer_global.into();
    let calldata_pointer = context.build_load(calldata_pointer_pointer, "calldata_pointer");
    let calldata_pointer = context.build_gep(
        Pointer::new(
            context.byte_type(),
            calldata_pointer_pointer.address_space,
            calldata_pointer.into_pointer_value(),
        ),
        &[offset],
        context.field_type().as_basic_type_enum(),
        "calldata_pointer_with_offset",
    );
    let value = context.build_load(calldata_pointer, "calldata_value");
    Ok(value)
}

///
/// Translates the calldata size.
///
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let value = context.get_global_value(crate::eravm::GLOBAL_CALLDATA_SIZE)?;

    Ok(value)
}

///
/// Translates the calldata copy.
///
pub fn copy<'ctx, D>(
    context: &mut Context<'ctx, D>,
    destination_offset: inkwell::values::IntValue<'ctx>,
    source_offset: inkwell::values::IntValue<'ctx>,
    size: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        destination_offset,
        "calldata_copy_destination_pointer",
    );

    let calldata_pointer_global = context.get_global(crate::eravm::GLOBAL_CALLDATA_POINTER)?;
    let calldata_pointer_pointer = calldata_pointer_global.into();
    let calldata_pointer = context.build_load(calldata_pointer_pointer, "calldata_pointer");
    let source = context.build_gep(
        Pointer::new(
            context.byte_type(),
            calldata_pointer_pointer.address_space,
            calldata_pointer.into_pointer_value(),
        ),
        &[source_offset],
        context.byte_type().as_basic_type_enum(),
        "calldata_source_pointer",
    );

    context.build_memcpy(
        context.intrinsics().memory_copy_from_generic,
        destination,
        source,
        size,
        "calldata_copy_memcpy_from_child",
    );

    Ok(())
}
