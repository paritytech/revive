//! Translates the storage operations.

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::PolkaVMArgument;
use crate::PolkaVMLoadStorageWordFunction;
use crate::PolkaVMLoadTransientStorageWordFunction;
use crate::PolkaVMStoreStorageWordFunction;
use crate::PolkaVMStoreTransientStorageWordFunction;

/// Translates the storage load.
pub fn load<'ctx>(
    context: &mut Context<'ctx>,
    position: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMLoadStorageWordFunction as RuntimeFunction>::NAME;
    let declaration = <PolkaVMLoadStorageWordFunction as RuntimeFunction>::declaration(context);
    let arguments = [position.as_pointer(context)?.value.into()];
    Ok(context
        .build_call(declaration, &arguments, "storage_load")
        .unwrap_or_else(|| panic!("runtime function {name} should return a value")))
}

/// Translates the storage store.
pub fn store<'ctx>(
    context: &mut Context<'ctx>,
    position: &PolkaVMArgument<'ctx>,
    value: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<()> {
    let declaration = <PolkaVMStoreStorageWordFunction as RuntimeFunction>::declaration(context);
    let arguments = [
        position.as_pointer(context)?.value.into(),
        value.as_pointer(context)?.value.into(),
    ];
    context.build_call(declaration, &arguments, "storage_store");
    Ok(())
}

/// Translates the transient storage load.
pub fn transient_load<'ctx>(
    context: &mut Context<'ctx>,
    position: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMLoadTransientStorageWordFunction as RuntimeFunction>::NAME;
    let arguments = [position.as_pointer(context)?.value.into()];
    let declaration =
        <PolkaVMLoadTransientStorageWordFunction as RuntimeFunction>::declaration(context);
    Ok(context
        .build_call(declaration, &arguments, "transient_storage_load")
        .unwrap_or_else(|| panic!("runtime function {name} should return a value")))
}

/// Translates the transient storage store.
pub fn transient_store<'ctx>(
    context: &mut Context<'ctx>,
    position: &PolkaVMArgument<'ctx>,
    value: &PolkaVMArgument<'ctx>,
) -> anyhow::Result<()> {
    let declaration =
        <PolkaVMStoreTransientStorageWordFunction as RuntimeFunction>::declaration(context);
    let arguments = [
        position.as_pointer(context)?.value.into(),
        value.as_pointer(context)?.value.into(),
    ];
    context.build_call(declaration, &arguments, "transient_storage_store");
    Ok(())
}
