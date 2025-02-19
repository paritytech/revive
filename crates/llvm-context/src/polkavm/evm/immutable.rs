//! Translates the contract immutable operations.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

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
            let name = <runtime::immutable_data_load::ImmutableDataLoad as runtime::RuntimeFunction<D>>::FUNCTION_NAME;
            context.build_call(
                context
                    .get_function(name)
                    .expect("is always declared for runtime code")
                    .borrow()
                    .declaration(),
                &[],
                name,
            );
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
