//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

//use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
//use crate::PolkaVMDivisionFunction;
//use crate::PolkaVMRemainderFunction;
//use crate::PolkaVMSignedDivisionFunction;
//use crate::PolkaVMSignedRemainderFunction;

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
    let result_pointer = context.build_alloca_at_entry(context.word_type(), "div_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "operand_1_pointer");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "operand_2_pointer");

    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(operand_2_pointer, operand_2)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::DIV, arguments);
    context.build_load(result_pointer, "div_result")
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
    let result_pointer = context.build_alloca_at_entry(context.word_type(), "rem_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "operand_1_pointer");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "operand_2_pointer");

    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(operand_2_pointer, operand_2)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::MOD, arguments);
    context.build_load(result_pointer, "rem_result")
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
    let result_pointer = context.build_alloca_at_entry(context.word_type(), "sdiv_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "operand_1_pointer");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "operand_2_pointer");

    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(operand_2_pointer, operand_2)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::SDIV, arguments);
    context.build_load(result_pointer, "sdiv_result")
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
    let result_pointer = context.build_alloca_at_entry(context.word_type(), "srem_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "operand_1_pointer");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "operand_2_pointer");

    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(operand_2_pointer, operand_2)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::SMOD, arguments);
    context.build_load(result_pointer, "rsem_result")
}
