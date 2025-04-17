//! Translates the storage operations.

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::PolkaVMArgument;
use crate::PolkaVMLoadStorageWordFunction;
use crate::PolkaVMLoadTransientStorageWordFunction;
use crate::PolkaVMStoreStorageWordFunction;
use crate::PolkaVMStoreTransientStorageWordFunction;

/// Translates the storage load.
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: &PolkaVMArgument<'ctx>,
    assignment_pointer: &mut Option<inkwell::values::PointerValue<'ctx>>,
) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>>
where
    D: Dependency + Clone,
{
    let _name = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    match assignment_pointer.take() {
        Some(assignment_pointer) => {
            let arguments = [
                position.as_pointer(context)?.value.into(),
                assignment_pointer.into(),
            ];
            context.build_call(declaration, &arguments, "storage_load");
            Ok(None)
        }
        None => {
            let pointer = context.build_alloca_at_entry(context.word_type(), "pointer");
            let arguments = [
                position.as_pointer(context)?.value.into(),
                pointer.value.into(),
            ];
            context.build_call(declaration, &arguments, "storage_load");
            Ok(Some(context.build_load(pointer, "storage_value")?))
        }
    }
}

/// Translates the storage store.
pub fn store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: &PolkaVMArgument<'ctx>,
    value: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let declaration = <PolkaVMStoreStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [
        position.as_pointer(context)?.value.into(),
        value.as_pointer(context)?.value.into(),
    ];
    context.build_call(declaration, &arguments, "storage_store");
    Ok(())
}

/// Translates the transient storage load.
pub fn transient_load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMLoadTransientStorageWordFunction as RuntimeFunction<D>>::NAME;
    let arguments = [position.as_pointer(context)?.value.into()];
    let declaration =
        <PolkaVMLoadTransientStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &arguments, "transient_storage_load")
        .unwrap_or_else(|| panic!("runtime function {name} should return a value")))
}

/// Translates the transient storage store.
pub fn transient_store<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: &PolkaVMArgument<'ctx>,
    value: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let declaration =
        <PolkaVMStoreTransientStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [
        position.as_pointer(context)?.value.into(),
        value.as_pointer(context)?.value.into(),
    ];
    context.build_call(declaration, &arguments, "transient_storage_store");
    Ok(())
}
