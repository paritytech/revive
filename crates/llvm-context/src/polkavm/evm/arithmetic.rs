//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::PolkaVMDivisionFunction;
use crate::PolkaVMRemainderFunction;
use crate::PolkaVMSignedDivisionFunction;
use crate::PolkaVMSignedRemainderFunction;

/// Translates the arithmetic addition.
pub fn addition<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_int_add(operand_1, operand_2, "addition_result")?
        .as_basic_value_enum())
}

/// Translates the arithmetic subtraction.
pub fn subtraction<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_int_sub(operand_1, operand_2, "subtraction_result")?
        .as_basic_value_enum())
}

/// Translates the arithmetic multiplication.
pub fn multiplication<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_int_mul(operand_1, operand_2, "multiplication_result")?
        .as_basic_value_enum())
}

/// Translates the arithmetic division.
pub fn division<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMDivisionFunction as RuntimeFunction>::NAME;
    let declaration = <PolkaVMDivisionFunction as RuntimeFunction>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "div")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the arithmetic remainder.
pub fn remainder<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMRemainderFunction as RuntimeFunction>::NAME;
    let declaration = <PolkaVMRemainderFunction as RuntimeFunction>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "rem")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the signed arithmetic division.
/// Two differences between the EVM and LLVM IR:
/// 1. In case of division by zero, 0 is returned.
/// 2. In case of overflow, the first argument is returned.
pub fn division_signed<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMSignedDivisionFunction as RuntimeFunction>::NAME;
    let declaration = <PolkaVMSignedDivisionFunction as RuntimeFunction>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "sdiv")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}

/// Translates the signed arithmetic remainder.
pub fn remainder_signed<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let name = <PolkaVMSignedRemainderFunction as RuntimeFunction>::NAME;
    let declaration = <PolkaVMSignedRemainderFunction as RuntimeFunction>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "srem")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value",)))
}
