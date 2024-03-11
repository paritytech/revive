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
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let overflow_block = context.append_basic_block("shift_left_overflow");
    let non_overflow_block = context.append_basic_block("shift_left_non_overflow");
    let join_block = context.append_basic_block("shift_left_join");

    let result_pointer = context.build_alloca(context.field_type(), "shift_left_result_pointer");
    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.field_const((era_compiler_common::BIT_LENGTH_FIELD - 1) as u64),
        "shift_left_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    context.build_store(result_pointer, context.field_const(0))?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value =
        context
            .builder()
            .build_left_shift(value, shift, "shift_left_non_overflow_result")?;
    context.build_store(result_pointer, value)?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    context.build_load(result_pointer, "shift_left_result")
}

///
/// Translates the bitwise shift right.
///
pub fn shift_right<'ctx, D>(
    context: &mut Context<'ctx, D>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let overflow_block = context.append_basic_block("shift_right_overflow");
    let non_overflow_block = context.append_basic_block("shift_right_non_overflow");
    let join_block = context.append_basic_block("shift_right_join");

    let result_pointer = context.build_alloca(context.field_type(), "shift_right_result_pointer");
    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.field_const((era_compiler_common::BIT_LENGTH_FIELD - 1) as u64),
        "shift_right_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    context.build_store(result_pointer, context.field_const(0))?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value = context.builder().build_right_shift(
        value,
        shift,
        false,
        "shift_right_non_overflow_result",
    )?;
    context.build_store(result_pointer, value)?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    context.build_load(result_pointer, "shift_right_result")
}

///
/// Translates the arithmetic bitwise shift right.
///
pub fn shift_right_arithmetic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let overflow_block = context.append_basic_block("shift_right_arithmetic_overflow");
    let overflow_positive_block =
        context.append_basic_block("shift_right_arithmetic_overflow_positive");
    let overflow_negative_block =
        context.append_basic_block("shift_right_arithmetic_overflow_negative");
    let non_overflow_block = context.append_basic_block("shift_right_arithmetic_non_overflow");
    let join_block = context.append_basic_block("shift_right_arithmetic_join");

    let result_pointer = context.build_alloca(
        context.field_type(),
        "shift_right_arithmetic_result_pointer",
    );
    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.field_const((era_compiler_common::BIT_LENGTH_FIELD - 1) as u64),
        "shift_right_arithmetic_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    let sign_bit = context.builder().build_right_shift(
        value,
        context.field_const((era_compiler_common::BIT_LENGTH_FIELD - 1) as u64),
        false,
        "shift_right_arithmetic_sign_bit",
    )?;
    let condition_is_negative = context.builder().build_int_truncate_or_bit_cast(
        sign_bit,
        context.bool_type(),
        "shift_right_arithmetic_sign_bit_truncated",
    )?;
    context.build_conditional_branch(
        condition_is_negative,
        overflow_negative_block,
        overflow_positive_block,
    )?;

    context.set_basic_block(overflow_positive_block);
    context.build_store(result_pointer, context.field_const(0))?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(overflow_negative_block);
    context.build_store(result_pointer, context.field_type().const_all_ones())?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value = context.builder().build_right_shift(
        value,
        shift,
        true,
        "shift_right_arithmetic_non_overflow_result",
    )?;
    context.build_store(result_pointer, value)?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    context.build_load(result_pointer, "shift_right_arithmetic_result")
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
