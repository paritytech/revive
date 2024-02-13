//!
//! Translates the math instructions of the EraVM Yul extension.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Performs a multiplication, returning the higher register, that is the overflown part.
///
pub fn multiplication_512<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let operand_1_extended = context.builder().build_int_z_extend_or_bit_cast(
        operand_1,
        context.integer_type(era_compiler_common::BIT_LENGTH_FIELD * 2),
        "multiplication_512_operand_1_extended",
    )?;
    let operand_2_extended = context.builder().build_int_z_extend_or_bit_cast(
        operand_2,
        context.integer_type(era_compiler_common::BIT_LENGTH_FIELD * 2),
        "multiplication_512_operand_2_extended",
    )?;
    let result_extended = context.builder().build_int_mul(
        operand_1_extended,
        operand_2_extended,
        "multiplication_512_result_extended",
    )?;
    let result_shifted = context.builder().build_right_shift(
        result_extended,
        context.integer_const(
            era_compiler_common::BIT_LENGTH_FIELD * 2,
            era_compiler_common::BIT_LENGTH_FIELD as u64,
        ),
        false,
        "multiplication_512_result_shifted",
    )?;
    let result = context.builder().build_int_truncate_or_bit_cast(
        result_shifted,
        context.field_type(),
        "multiplication_512_result",
    )?;

    Ok(result.as_basic_value_enum())
}
