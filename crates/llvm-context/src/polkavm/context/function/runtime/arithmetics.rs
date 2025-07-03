//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// Implements the ADD operator according to the EVM specification.
pub struct Addition;

impl<D> RuntimeFunction<D> for Addition
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_addition";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        Ok(Some(
            context
                .builder()
                .build_int_add(operand_1, operand_2, "ADD")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for Addition
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the SUB operator according to the EVM specification.
pub struct Subtraction;

impl<D> RuntimeFunction<D> for Subtraction
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_subtraction";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        Ok(Some(
            context
                .builder()
                .build_int_sub(operand_1, operand_2, "SUB")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for Subtraction
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the MUL operator according to the EVM specification.
pub struct Multiplication;

impl<D> RuntimeFunction<D> for Multiplication
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_multiplication";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        Ok(Some(
            context
                .builder()
                .build_int_mul(operand_1, operand_2, "MUL")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for Multiplication
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the division operator according to the EVM specification.
pub struct Division;

impl<D> RuntimeFunction<D> for Division
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_division";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        wrapped_division(context, operand_2, || {
            Ok(context
                .builder()
                .build_int_unsigned_div(operand_1, operand_2, "DIV")?)
        })
        .map(Into::into)
    }
}

impl<D> WriteLLVM<D> for Division
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the signed division operator according to the EVM specification.
pub struct SignedDivision;

impl<D> RuntimeFunction<D> for SignedDivision
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_signed_division";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

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
        Ok(Some(result.as_basic_value()))
    }
}

impl<D> WriteLLVM<D> for SignedDivision
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the remainder operator according to the EVM specification.
pub struct Remainder;

impl<D> RuntimeFunction<D> for Remainder
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_remainder";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        wrapped_division(context, operand_2, || {
            Ok(context
                .builder()
                .build_int_unsigned_rem(operand_1, operand_2, "MOD")?)
        })
        .map(Into::into)
    }
}

impl<D> WriteLLVM<D> for Remainder
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the signed remainder operator according to the EVM specification.
pub struct SignedRemainder;

impl<D> RuntimeFunction<D> for SignedRemainder
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_signed_remainder";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let operand_1 = Self::paramater(context, 0).into_int_value();
        let operand_2 = Self::paramater(context, 1).into_int_value();

        wrapped_division(context, operand_2, || {
            Ok(context
                .builder()
                .build_int_signed_rem(operand_1, operand_2, "SMOD")?)
        })
        .map(Into::into)
    }
}

impl<D> WriteLLVM<D> for SignedRemainder
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
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
