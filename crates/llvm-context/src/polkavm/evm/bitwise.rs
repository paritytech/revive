//! Translates the bitwise operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;

/// Translates the bitwise OR.
pub fn or<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_or(operand_1, operand_2, "or_result")?
        .as_basic_value_enum())
}

/// Translates the bitwise XOR.
pub fn xor<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_xor(operand_1, operand_2, "xor_result")?
        .as_basic_value_enum())
}

/// Translates the bitwise AND.
pub fn and<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .builder()
        .build_and(operand_1, operand_2, "and_result")?
        .as_basic_value_enum())
}

/// Translates the bitwise shift left.
pub fn shift_left<'ctx>(
    context: &mut Context<'ctx>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let overflow_block = context.append_basic_block("shift_left_overflow");
    let non_overflow_block = context.append_basic_block("shift_left_non_overflow");
    let join_block = context.append_basic_block("shift_left_join");

    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
        "shift_left_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value =
        context
            .builder()
            .build_left_shift(value, shift, "shift_left_non_overflow_result")?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    let result = context
        .builder()
        .build_phi(context.word_type(), "shift_left_value")?;
    result.add_incoming(&[
        (&value, non_overflow_block),
        (&context.word_const(0), overflow_block),
    ]);
    Ok(result.as_basic_value())
}

/// Translates the bitwise shift right.
pub fn shift_right<'ctx>(
    context: &mut Context<'ctx>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let overflow_block = context.append_basic_block("shift_right_overflow");
    let non_overflow_block = context.append_basic_block("shift_right_non_overflow");
    let join_block = context.append_basic_block("shift_right_join");

    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
        "shift_right_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value = context.builder().build_right_shift(
        value,
        shift,
        false,
        "shift_right_non_overflow_result",
    )?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    let result = context
        .builder()
        .build_phi(context.word_type(), "shift_right_value")?;
    result.add_incoming(&[
        (&value, non_overflow_block),
        (&context.word_const(0), overflow_block),
    ]);
    Ok(result.as_basic_value())
}

/// Translates the arithmetic bitwise shift right.
pub fn shift_right_arithmetic<'ctx>(
    context: &mut Context<'ctx>,
    shift: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let overflow_block = context.append_basic_block("shift_right_arithmetic_overflow");
    let overflow_positive_block =
        context.append_basic_block("shift_right_arithmetic_overflow_positive");
    let overflow_negative_block =
        context.append_basic_block("shift_right_arithmetic_overflow_negative");
    let non_overflow_block = context.append_basic_block("shift_right_arithmetic_non_overflow");
    let join_block = context.append_basic_block("shift_right_arithmetic_join");

    let condition_is_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::UGT,
        shift,
        context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
        "shift_right_arithmetic_is_overflow",
    )?;
    context.build_conditional_branch(condition_is_overflow, overflow_block, non_overflow_block)?;

    context.set_basic_block(overflow_block);
    let sign_bit = context.builder().build_right_shift(
        value,
        context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
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
    context.build_unconditional_branch(join_block);

    context.set_basic_block(overflow_negative_block);
    context.build_unconditional_branch(join_block);

    context.set_basic_block(non_overflow_block);
    let value = context.builder().build_right_shift(
        value,
        shift,
        true,
        "shift_right_arithmetic_non_overflow_result",
    )?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    let result = context
        .builder()
        .build_phi(context.word_type(), "shift_arithmetic_right_value")?;
    result.add_incoming(&[
        (&value, non_overflow_block),
        (
            &context.word_type().const_all_ones(),
            overflow_negative_block,
        ),
        (&context.word_const(0), overflow_positive_block),
    ]);
    Ok(result.as_basic_value())
}

/// Translates the `byte` instruction, extracting the byte of `operand_2`
/// found at index `operand_1`, starting from the most significant bit.
///
/// Builds a logical `and` with a corresponding bit mask.
///
/// Because this opcode returns zero on overflows, the index `operand_1`
/// is checked for overflow. On overflow, the mask will be all zeros,
/// resulting in a branchless implementation.
pub fn byte<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    const MAX_INDEX_BYTES: u64 = 31;

    let is_overflow_bit = context.builder().build_int_compare(
        inkwell::IntPredicate::ULE,
        operand_1,
        context.word_const(MAX_INDEX_BYTES),
        "is_overflow_bit",
    )?;
    let is_overflow_byte = context.builder().build_int_z_extend(
        is_overflow_bit,
        context.byte_type(),
        "is_overflow_byte",
    )?;
    let mask_byte = context.builder().build_int_mul(
        context.byte_type().const_all_ones(),
        is_overflow_byte,
        "mask_byte",
    )?;
    let mask_byte_word =
        context
            .builder()
            .build_int_z_extend(mask_byte, context.word_type(), "mask_byte_word")?;

    let index_truncated =
        context
            .builder()
            .build_int_truncate(operand_1, context.byte_type(), "index_truncated")?;
    let index_in_bits = context.builder().build_int_mul(
        index_truncated,
        context
            .byte_type()
            .const_int(revive_common::BIT_LENGTH_BYTE as u64, false),
        "index_in_bits",
    )?;
    let index_from_most_significant_bit = context.builder().build_int_sub(
        context.byte_type().const_int(
            MAX_INDEX_BYTES * revive_common::BIT_LENGTH_BYTE as u64,
            false,
        ),
        index_in_bits,
        "index_from_msb",
    )?;
    let index_extended = context.builder().build_int_z_extend(
        index_from_most_significant_bit,
        context.word_type(),
        "index",
    )?;

    let mask = context
        .builder()
        .build_left_shift(mask_byte_word, index_extended, "mask")?;
    let masked_value = context.builder().build_and(operand_2, mask, "masked")?;
    let byte = context
        .builder()
        .build_right_shift(masked_value, index_extended, false, "byte")?;

    Ok(byte.as_basic_value_enum())
}
