//! Translates the arithmetic operations.

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::PolkaVMAdditionFunction;
use crate::PolkaVMDivisionFunction;
use crate::PolkaVMMultiplicationFunction;
use crate::PolkaVMRemainderFunction;
use crate::PolkaVMSignedDivisionFunction;
use crate::PolkaVMSignedRemainderFunction;
use crate::PolkaVMSubstractionFunction;

/// Translates the arithmetic addition.
pub fn addition<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMAdditionFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMAdditionFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "SUB")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the arithmetic subtraction.
pub fn subtraction<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMSubstractionFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMSubstractionFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "SUB")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the arithmetic multiplication.
pub fn multiplication<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMMultiplicationFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMMultiplicationFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "MUL")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the arithmetic division.
pub fn division<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMDivisionFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMDivisionFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "div")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the arithmetic remainder.
pub fn remainder<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMRemainderFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMRemainderFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "rem")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the signed arithmetic division.
/// Two differences between the EVM and LLVM IR:
/// 1. In case of division by zero, 0 is returned.
/// 2. In case of overflow, the first argument is returned.
pub fn division_signed<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMSignedDivisionFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMSignedDivisionFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "sdiv")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the signed arithmetic remainder.
pub fn remainder_signed<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMSignedRemainderFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMSignedRemainderFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "srem")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}
