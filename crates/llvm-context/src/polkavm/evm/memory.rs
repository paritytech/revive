//! Translates the heap memory operations.

use inkwell::values::BasicValue;
use revive_common::BYTE_LENGTH_BYTE;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;

/// Translates the `msize` instruction.
pub fn msize<'ctx>(
    context: &mut Context<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_int_z_extend(
            context.build_msize()?,
            context.word_type(),
            "heap_size_extended",
        )?
        .as_basic_value_enum())
}

/// Translates the `mload` instruction.
/// Uses the main heap.
pub fn load<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let pointer = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.word_type(),
        offset,
        "memory_load_pointer",
    );
    context.build_load(pointer, "memory_load_result")
}

/// Translates the `mstore` instruction.
/// Uses the main heap.
pub fn store<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let pointer = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.word_type(),
        offset,
        "memory_store_pointer",
    );
    context.build_store(pointer, value)?;
    Ok(())
}

/// Translates the `mstore8` instruction.
/// Uses the main heap.
pub fn store_byte<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
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
    let pointer = context.build_sbrk(
        pointer.to_int(context),
        context.xlen_type().const_int(1, false),
    )?;
    context
        .builder()
        .build_store(pointer, value)?
        .set_alignment(BYTE_LENGTH_BYTE as u32)
        .expect("Alignment is valid");
    Ok(())
}

/// Translates the `mload` instruction without byte-swapping.
/// Used for internal memory operations that don't escape to external code.
pub fn load_native<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    context.build_load_native(offset)
}

/// Translates the `mstore` instruction without byte-swapping.
/// Used for internal memory operations that don't escape to external code.
pub fn store_native<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    context.build_store_native(offset, value)
}

/// Translates the `mload` instruction without byte-swapping, inlined.
/// This version inlines the load directly without a function call,
/// saving both call overhead and function body size when not all
/// memory accesses need native mode.
pub fn load_native_inline<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let length = context
        .xlen_type()
        .const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
    let pointer = context.build_heap_gep(offset, length)?;
    let value = context
        .builder()
        .build_load(context.word_type(), pointer.value, "native_load")?;
    context
        .basic_block()
        .get_last_instruction()
        .expect("Always exists")
        .set_alignment(BYTE_LENGTH_BYTE as u32)
        .expect("Alignment is valid");
    Ok(value)
}

/// Translates the `mstore` instruction without byte-swapping, inlined.
/// This version inlines the store directly without a function call,
/// saving both call overhead and function body size when not all
/// memory accesses need native mode.
pub fn store_native_inline<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let length = context
        .xlen_type()
        .const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
    let pointer = context.build_heap_gep(offset, length)?;
    context
        .builder()
        .build_store(pointer.value, value)?
        .set_alignment(BYTE_LENGTH_BYTE as u32)
        .expect("Alignment is valid");
    Ok(())
}
