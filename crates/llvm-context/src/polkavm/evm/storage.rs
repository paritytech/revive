//! Translates the storage operations.

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the storage load.
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let mut slot_ptr = context.build_alloca_at_entry(context.word_type(), "slot_pointer");
    slot_ptr.address_space = AddressSpace::Storage;
    context.builder().build_store(slot_ptr.value, position)?;
    context.build_load(slot_ptr, "storage_load_value")
}

/// Translates the storage store.
pub fn store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let mut slot_ptr = context.build_alloca_at_entry(context.word_type(), "slot_pointer");
    slot_ptr.address_space = AddressSpace::Storage;
    context.builder().build_store(slot_ptr.value, position)?;
    context.build_store(slot_ptr, value)?;
    Ok(())
}

/// Translates the transient storage load.
pub fn transient_load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let mut slot_ptr = context.build_alloca_at_entry(context.word_type(), "slot_pointer");
    slot_ptr.address_space = AddressSpace::TransientStorage;
    context.builder().build_store(slot_ptr.value, position)?;
    context.build_load(slot_ptr, "transient_storage_load_value")
}

/// Translates the transient storage store.
pub fn transient_store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let mut slot_ptr = context.build_alloca_at_entry(context.word_type(), "slot_pointer");
    slot_ptr.address_space = AddressSpace::TransientStorage;
    context.builder().build_store(slot_ptr.value, position)?;
    context.build_store(slot_ptr, value)?;
    Ok(())
}
