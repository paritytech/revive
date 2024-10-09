//! Translates the contract immutable operations.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::{runtime_api, Dependency};

/// Translates the contract immutable load.
///
/// In deploy code the values are read from the stack.
///
/// In runtime code they are loaded lazily with the `get_immutable_data` syscall.
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
        Some(CodeType::Deploy) => load_from_memory(context, index),
        Some(CodeType::Runtime) => {
            let immutable_data_size_pointer = context
                .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_SIZE)?
                .value
                .as_pointer_value();
            let immutable_data_size = context.build_load(
                Pointer::new(
                    context.xlen_type(),
                    AddressSpace::Stack,
                    immutable_data_size_pointer,
                ),
                "immutable_data_size_load",
            )?;

            let load_immutable_data_block = context.append_basic_block("load_immutables_block");
            let join_load_block = context.append_basic_block("join_load_block");
            let immutable_data_size_is_zero = context.builder().build_int_compare(
                inkwell::IntPredicate::EQ,
                context.xlen_type().const_zero(),
                immutable_data_size.into_int_value(),
                "immutable_data_size_is_zero",
            )?;
            context.build_conditional_branch(
                immutable_data_size_is_zero,
                join_load_block,
                load_immutable_data_block,
            )?;

            context.set_basic_block(load_immutable_data_block);
            let output_pointer = context
                .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
                .value
                .as_pointer_value();
            context.build_runtime_call(
                runtime_api::imports::GET_IMMUTABLE_DATA,
                &[
                    context
                        .builder()
                        .build_ptr_to_int(output_pointer, context.xlen_type(), "ptr_to_xlen")?
                        .into(),
                    context
                        .builder()
                        .build_ptr_to_int(
                            immutable_data_size_pointer,
                            context.xlen_type(),
                            "ptr_to_xlen",
                        )?
                        .into(),
                ],
            );
            // todo: check out length
            context.builder().build_store(
                immutable_data_size_pointer,
                context.xlen_type().const_zero(),
            )?;
            context.build_unconditional_branch(join_load_block);

            context.set_basic_block(join_load_block);
            load_from_memory(context, index)
        }
    }
}

/// Translates the contract immutable store.
///
/// In deploy code the values are written to the stack at the predefined offset,
/// being prepared for storing them using the `set_immutable_data` syscall.
///
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
            let immutable_data_pointer = context
                .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
                .value
                .as_pointer_value();
            let immutable_pointer = context.build_gep(
                Pointer::new(
                    context.word_type(),
                    AddressSpace::Stack,
                    immutable_data_pointer,
                ),
                &[index],
                context.word_type().as_basic_type_enum(),
                "immutable_variable_pointer",
            );
            context.build_store(immutable_pointer, value)
        }
        Some(CodeType::Runtime) => {
            anyhow::bail!("Immutable writes are not available in the runtime code");
        }
    }
}

pub fn load_from_memory<'ctx, D>(
    context: &mut Context<'ctx, D>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let immutable_data_pointer = context
        .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
        .value
        .as_pointer_value();
    let immutable_pointer = context.build_gep(
        Pointer::new(
            context.word_type(),
            AddressSpace::Stack,
            immutable_data_pointer,
        ),
        &[index],
        context.word_type().as_basic_type_enum(),
        "immutable_variable_pointer",
    );
    context.build_load(immutable_pointer, "immutable_value")
}
