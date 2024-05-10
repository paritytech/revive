//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

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
    wrapped_division(context, operand_2, || {
        Ok(context
            .builder()
            .build_int_unsigned_div(operand_1, operand_2, "DIV")?)
    })
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
    wrapped_division(context, operand_2, || {
        Ok(context
            .builder()
            .build_int_unsigned_rem(operand_1, operand_2, "MOD")?)
    })
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
    assert_eq!(
        operand_2.get_type().get_bit_width(),
        revive_common::BIT_LENGTH_WORD as u32
    );

    let block_calculate = context.append_basic_block("calculate");
    let block_overflow = context.append_basic_block("overflow");
    let block_select = context.append_basic_block("select_result");
    let block_origin = context.basic_block();
    context.builder().build_switch(
        operand_2,
        block_calculate,
        &[
            (context.word_type().const_zero(), block_select),
            (context.word_type().const_all_ones(), block_overflow),
        ],
    )?;

    context.set_basic_block(block_calculate);
    let quotient = context
        .builder()
        .build_int_signed_div(operand_1, operand_2, "SDIV")?;
    context.build_unconditional_branch(block_select);

    context.set_basic_block(block_overflow);
    let max_uint = context.builder().build_int_z_extend(
        context
            .integer_type(revive_common::BIT_LENGTH_WORD - 1)
            .const_all_ones(),
        context.word_type(),
        "max_uint",
    )?;
    let is_operand_1_overflow = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        operand_1,
        context.builder().build_int_neg(max_uint, "min_uint")?,
        "is_operand_1_overflow",
    )?;
    context.build_conditional_branch(is_operand_1_overflow, block_select, block_calculate)?;

    context.set_basic_block(block_select);
    let result = context.builder().build_phi(context.word_type(), "result")?;
    result.add_incoming(&[
        (&operand_1, block_overflow),
        (&context.word_const(0), block_origin),
        (&quotient.as_basic_value_enum(), block_calculate),
    ]);
    Ok(result.as_basic_value())
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
    wrapped_division(context, operand_2, || {
        Ok(context
            .builder()
            .build_int_signed_rem(operand_1, operand_2, "SMOD")?)
    })
}

/// Wrap division operations so that zero will be returned if the
/// denominator is zero (see also Ethereum YP Appendix H.2).
///
/// The closure is expected to calculate and return the quotient.
///
/// The result is either the calculated quotient or zero,
///  selected at runtime.
fn wrapped_division<'ctx, D, F, T>(
    context: &Context<'ctx, D>,
    denominator: inkwell::values::IntValue<'ctx>,
    f: F,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
    F: FnOnce() -> anyhow::Result<T>,
    T: inkwell::values::IntMathValue<'ctx>,
{
    assert_eq!(
        denominator.get_type().get_bit_width(),
        revive_common::BIT_LENGTH_WORD as u32
    );

    let block_calculate = context.append_basic_block("calculate");
    let block_select = context.append_basic_block("select");
    let block_origin = context.basic_block();
    context.builder().build_switch(
        denominator,
        block_calculate,
        &[(context.word_const(0), block_select)],
    )?;

    context.set_basic_block(block_calculate);
    let calculated_value = f()?.as_basic_value_enum();
    context.build_unconditional_branch(block_select);

    context.set_basic_block(block_select);
    let result = context.builder().build_phi(context.word_type(), "result")?;
    result.add_incoming(&[
        (&context.word_const(0), block_origin),
        (&calculated_value, block_calculate),
    ]);
    Ok(result.as_basic_value())
}
