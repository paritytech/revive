//! Translates the bitwise operations.

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::{
    PolkaVMAndFunction, PolkaVMByteFunction, PolkaVMOrFunction, PolkaVMSarFunction,
    PolkaVMShlFunction, PolkaVMShrFunction, PolkaVMXorFunction,
};

/// Translates the bitwise OR.
pub fn or<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMOrFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMOrFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "OR")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the bitwise XOR.
pub fn xor<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMXorFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMXorFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "XOR")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the bitwise AND.
pub fn and<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMAndFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMAndFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "AND")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the bitwise shift left.
pub fn shift_left<'ctx, D>(
    context: &mut Context<'ctx, D>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMShlFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMShlFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[shift.into(), value.into()], "SHL")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the bitwise shift right.
pub fn shift_right<'ctx, D>(
    context: &mut Context<'ctx, D>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMShrFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMShrFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[shift.into(), value.into()], "SHR")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the arithmetic bitwise shift right.
pub fn shift_right_arithmetic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMSarFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMSarFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[shift.into(), value.into()], "SHR")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}

/// Translates the `byte` instruction, extracting the byte of `operand_2`
/// found at index `operand_1`, starting from the most significant bit.
///
/// Builds a logical `and` with a corresponding bit mask.
///
/// Because this opcode returns zero on overflows, the index `operand_1`
/// is checked for overflow. On overflow, the mask will be all zeros,
/// resulting in a branchless implementation.
pub fn byte<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMByteFunction as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMByteFunction as RuntimeFunction<D>>::declaration(context);
    Ok(context
        .build_call(declaration, &[operand_1.into(), operand_2.into()], "BYTE")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
}
