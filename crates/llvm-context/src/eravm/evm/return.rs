//!
//! Translates the transaction return operations.
//!

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::code_type::CodeType;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the `return` instruction.
///
/// Unlike in EVM, zkSync constructors return the array of contract immutables.
///
pub fn r#return<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    match context.code_type() {
        None => {
            anyhow::bail!("Return is not available if the contract part is undefined");
        }
        Some(CodeType::Deploy) => {
            let immutables_offset_pointer = Pointer::new_with_offset(
                context,
                AddressSpace::HeapAuxiliary,
                context.field_type(),
                context.field_const(crate::eravm::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA),
                "immutables_offset_pointer",
            );
            context.build_store(
                immutables_offset_pointer,
                context.field_const(era_compiler_common::BYTE_LENGTH_FIELD as u64),
            );

            let immutables_number_pointer = Pointer::new_with_offset(
                context,
                AddressSpace::HeapAuxiliary,
                context.field_type(),
                context.field_const(
                    crate::eravm::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA
                        + (era_compiler_common::BYTE_LENGTH_FIELD as u64),
                ),
                "immutables_number_pointer",
            );
            let immutable_values_size = context.immutables_size()?;
            context.build_store(
                immutables_number_pointer,
                context.field_const(
                    (immutable_values_size / era_compiler_common::BYTE_LENGTH_FIELD) as u64,
                ),
            );
            let immutables_size = context.builder().build_int_mul(
                context.field_const(immutable_values_size as u64),
                context.field_const(2),
                "immutables_size",
            )?;
            let return_data_length = context.builder().build_int_add(
                immutables_size,
                context.field_const((era_compiler_common::BYTE_LENGTH_FIELD * 2) as u64),
                "return_data_length",
            )?;

            context.build_exit(
                context.integer_const(32, 0),
                context.field_const(crate::eravm::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA),
                return_data_length,
            )?;
        }
        Some(CodeType::Runtime) => {
            context.build_exit(context.integer_const(32, 0), offset, length)?;
        }
    }

    Ok(())
}

///
/// Translates the `revert` instruction.
///
pub fn revert<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    context.build_exit(context.integer_const(32, 1), offset, length)
}

///
/// Translates the `stop` instruction.
///
/// Is the same as `return(0, 0)`.
///
pub fn stop<D>(context: &mut Context<D>) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    r#return(
        context,
        context.integer_const(32, 0),
        context.integer_const(32, 0),
    )
}

///
/// Translates the `invalid` instruction.
///
/// Burns all gas using an out-of-bounds memory store, causing a panic.
///
pub fn invalid<D>(context: &mut Context<D>) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    crate::eravm::evm::memory::store(
        context,
        context.field_type().const_all_ones(),
        context.field_const(0),
    )?;
    context.build_call(context.intrinsics().trap, &[], "invalid_trap");
    context.build_unreachable();
    Ok(())
}
