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
    WrappedDivision::new(context, operand_2)?.with(|| {
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
    WrappedDivision::new(context, operand_2)?.with(|| {
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
    WrappedDivision::new(context, operand_2)?.with(|| {
        let block_no_overflow = context.append_basic_block("no_overflow");
        let block_operand_1_overflow = context.append_basic_block("operand_1_overflow");
        let block_select_quotient = context.append_basic_block("block_select_quotient");

        let max_uint = context.builder().build_int_z_extend(
            context
                .integer_type(revive_common::BIT_LENGTH_WORD - 1)
                .const_all_ones(),
            context.word_type(),
            "constant_zext_max_uint",
        )?;
        let is_operand_1_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            operand_1,
            context.builder().build_int_neg(max_uint, "min_uint")?,
            "is_operand_1_overflow",
        )?;
        context.build_conditional_branch(
            is_operand_1_overflow,
            block_operand_1_overflow,
            block_no_overflow,
        )?;

        context.set_basic_block(block_operand_1_overflow);
        let is_operand_2_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            operand_2,
            context.word_type().const_all_ones(),
            "is_operand_2_overflow",
        )?;
        context.build_conditional_branch(
            is_operand_2_overflow,
            block_select_quotient,
            block_no_overflow,
        )?;

        context.set_basic_block(block_no_overflow);
        let quotient = context
            .builder()
            .build_int_signed_div(operand_1, operand_2, "SDIV")?;
        context.build_unconditional_branch(block_select_quotient);

        context.set_basic_block(block_select_quotient);
        let phi_value = context
            .builder()
            .build_phi(context.word_type(), "phi_quotient")?;
        phi_value.add_incoming(&[
            (&quotient.as_basic_value_enum(), block_no_overflow),
            (&operand_1, block_operand_1_overflow),
        ]);
        Ok(phi_value.as_basic_value().into_int_value())
    })
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
    WrappedDivision::new(context, operand_2)?.with(|| {
        Ok(context
            .builder()
            .build_int_signed_rem(operand_1, operand_2, "SMOD")?)
    })
}

/// Helper to wrap division operations so that zero will be returned
/// if the denominator is zero (see also Ethereum YP Appendix H.2).
struct WrappedDivision<'a, 'ctx, D: Dependency + Clone> {
    context: &'a Context<'ctx, D>,
    block_origin: inkwell::basic_block::BasicBlock<'ctx>,
    block_calculate: inkwell::basic_block::BasicBlock<'ctx>,
    block_select: inkwell::basic_block::BasicBlock<'ctx>,
}

impl<'a, 'ctx, D: Dependency + Clone> WrappedDivision<'a, 'ctx, D> {
    /// Create a new wrapped division (inserts a switch on the denominator).
    fn new(
        context: &'a Context<'ctx, D>,
        denominator: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<Self> {
        assert_eq!(
            denominator.get_type().get_bit_width(),
            revive_common::BIT_LENGTH_WORD as u32
        );

        let block_calculate = context.append_basic_block("calculate");
        let block_select = context.append_basic_block("select");
        context.builder().build_switch(
            denominator,
            block_calculate,
            &[(context.word_const(0), block_select)],
        )?;

        Ok(Self {
            context,
            block_origin: context.basic_block(),
            block_calculate,
            block_select,
        })
    }

    /// Insert code to calculate the operation.
    ///
    /// The closure is expected to calculate and return the quotient.
    ///
    /// The returned value is either the calculated quotient or zero, selected at runtime.
    fn with<T, F>(self, f: F) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
    where
        F: FnOnce() -> anyhow::Result<T>,
        T: inkwell::values::IntMathValue<'ctx>,
    {
        self.context.set_basic_block(self.block_calculate);
        let calculated_value = f()?.as_basic_value_enum();
        let calculated_value_incoming_block = self.context.basic_block();
        self.context.build_unconditional_branch(self.block_select);

        self.context.set_basic_block(self.block_select);
        let phi_value = self
            .context
            .builder()
            .build_phi(self.context.word_type(), "phi_result")?;
        phi_value.add_incoming(&[
            (&self.context.word_const(0), self.block_origin),
            (&calculated_value, calculated_value_incoming_block),
        ]);
        Ok(phi_value.as_basic_value())
    }
}
