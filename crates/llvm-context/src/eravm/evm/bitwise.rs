//!
//! Translates the bitwise operations.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates the bitwise OR.
///
pub fn or<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_or(operand_1, operand_2, "or_result")?
        .as_basic_value_enum())
}

///
/// Translates the bitwise XOR.
///
pub fn xor<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_xor(operand_1, operand_2, "xor_result")?
        .as_basic_value_enum())
}

///
/// Translates the bitwise AND.
///
pub fn and<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_and(operand_1, operand_2, "and_result")?
        .as_basic_value_enum())
}

///
/// Translates the bitwise shift left.
///
pub fn shift_left<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_left_shift(operand_1, operand_2, "shl")?
        .into())
}

///
/// Translates the bitwise shift right.
///
pub fn shift_right<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_right_shift(operand_1, operand_2, false, "shr")?
        .into())
}

///
/// Translates the arithmetic bitwise shift right.
///
pub fn shift_right_arithmetic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_right_shift(operand_1, operand_2, true, "ashr")?
        .into())
}

///
/// Translates the `byte` instruction.
///
pub fn byte<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .build_call(
            context.llvm_runtime().byte,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
            ],
            "byte_call",
        )
        .expect("Always exists"))
}
