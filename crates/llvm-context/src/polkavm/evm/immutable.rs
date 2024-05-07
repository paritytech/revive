//! Translates the contract immutable operations.

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the contract immutable load.
/// In the deploy code the values are read from the auxiliary heap.
/// In the runtime code they are requested from the system contract.
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    match context.code_type() {
        None => {
            anyhow::bail!("Immutables are not available if the contract part is undefined");
        }
        Some(CodeType::Deploy) => {
            let index_double = context.builder().build_int_mul(
                index,
                context.word_const(2),
                "immutable_load_index_double",
            )?;
            let offset_absolute = context.builder().build_int_add(
                index_double,
                context.word_const(
                    crate::polkavm::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA
                        + (3 * revive_common::BYTE_LENGTH_WORD) as u64,
                ),
                "immutable_offset_absolute",
            )?;
            let immutable_pointer = Pointer::new_with_offset(
                context,
                AddressSpace::HeapAuxiliary,
                context.word_type(),
                offset_absolute,
                "immutable_pointer",
            );
            context.build_load(immutable_pointer, "immutable_value")
        }
        Some(CodeType::Runtime) => {
            todo!()
        }
    }
}

/// Translates the contract immutable store.
/// In the deploy code the values are written to the auxiliary heap at the predefined offset,
/// being prepared for returning to the system contract for saving.
/// Ignored in the runtime code.
pub fn store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    match context.code_type() {
        None => {
            anyhow::bail!("Immutables are not available if the contract part is undefined");
        }
        Some(CodeType::Deploy) => {
            let index_double = context.builder().build_int_mul(
                index,
                context.word_const(2),
                "immutable_load_index_double",
            )?;
            let index_offset_absolute = context.builder().build_int_add(
                index_double,
                context.word_const(
                    crate::polkavm::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA
                        + (2 * revive_common::BYTE_LENGTH_WORD) as u64,
                ),
                "index_offset_absolute",
            )?;
            let index_offset_pointer = Pointer::new_with_offset(
                context,
                AddressSpace::HeapAuxiliary,
                context.word_type(),
                index_offset_absolute,
                "immutable_index_pointer",
            );
            context.build_store(index_offset_pointer, index)?;

            let value_offset_absolute = context.builder().build_int_add(
                index_offset_absolute,
                context.word_const(revive_common::BYTE_LENGTH_WORD as u64),
                "value_offset_absolute",
            )?;
            let value_offset_pointer = Pointer::new_with_offset(
                context,
                AddressSpace::HeapAuxiliary,
                context.word_type(),
                value_offset_absolute,
                "immutable_value_pointer",
            );
            context.build_store(value_offset_pointer, value)?;

            Ok(())
        }
        Some(CodeType::Runtime) => {
            anyhow::bail!("Immutable writes are not available in the runtime code");
        }
    }
}
