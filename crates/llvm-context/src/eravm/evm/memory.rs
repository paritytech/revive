//!
//! Translates the heap memory operations.
//!

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `mload` instruction.
///
/// Uses the main heap.
///
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let pointer = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.field_type(),
        offset,
        "memory_load_pointer",
    );
    context.build_load(pointer, "memory_load_result")
}

///
/// Translates the `mstore` instruction.
///
/// Uses the main heap.
///
pub fn store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let pointer = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.field_type(),
        offset,
        "memory_store_pointer",
    );
    context.build_store(pointer, value)?;
    Ok(())
}

///
/// Translates the `mstore8` instruction.
///
/// Uses the main heap.
///
pub fn store_byte<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let byte_type = context.byte_type();
    let value = context
        .builder()
        .build_int_truncate(value, byte_type, "mstore8_value")?;
    let pointer = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        byte_type,
        offset,
        "mstore8_destination",
    );
    context.build_store(pointer, value)
}
