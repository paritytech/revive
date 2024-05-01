//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::eravm::context::Context;
use crate::eravm::Dependency;

/// Translates the arithmetic addition.
pub fn addition<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    Ok(context
        .builder()
        .build_int_add(operand_1, operand_2, "addition_result")?
        .as_basic_value_enum())
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
    Ok(context
        .builder()
        .build_int_sub(operand_1, operand_2, "subtraction_result")?
        .as_basic_value_enum())
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
    Ok(context
        .builder()
        .build_int_mul(operand_1, operand_2, "multiplication_result")?
        .as_basic_value_enum())
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
    Ok(context
        .builder()
        .build_int_unsigned_div(operand_1, operand_2, "udiv")?
        .into())
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
    Ok(context
        .build_call(
            context.llvm_runtime().r#mod,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
            ],
            "add_mod_call",
        )
        .expect("Always exists"))
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
    Ok(context
        .build_call(
            context.llvm_runtime().sdiv,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
            ],
            "add_mod_call",
        )
        .expect("Always exists"))
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
    Ok(context
        .build_call(
            context.llvm_runtime().smod,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
            ],
            "add_mod_call",
        )
        .expect("Always exists"))
}
