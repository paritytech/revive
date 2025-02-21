//! Translates the storage operations.

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::PolkaVMLoadStorageWordFunction;
use crate::PolkaVMStoreStorageWordFunction;

/// Translates the storage load.
pub fn load<'ctx, D>(
    context: &mut Context<'ctx, D>,
    position: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [context.xlen_type().const_zero().into(), position.into()];
    Ok(context
        .build_call(declaration, &arguments, "storage_load")
        .unwrap_or_else(|| panic!("runtime function {name} should return a value")))
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
    let declaration = <PolkaVMStoreStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [
        context.xlen_type().const_zero().into(),
        position.into(),
        value.into(),
    ];
    context.build_call(declaration, &arguments, "storage_store");
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
    let name = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMLoadStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [
        context.xlen_type().const_int(1, false).into(),
        position.into(),
    ];
    Ok(context
        .build_call(declaration, &arguments, "storage_load")
        .unwrap_or_else(|| panic!("runtime function {name} should return a value")))
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
    let declaration = <PolkaVMStoreStorageWordFunction as RuntimeFunction<D>>::declaration(context);
    let arguments = [
        context.xlen_type().const_int(1, false).into(),
        position.into(),
        value.into(),
    ];
    context.build_call(declaration, &arguments, "storage_store");
    Ok(())
}
