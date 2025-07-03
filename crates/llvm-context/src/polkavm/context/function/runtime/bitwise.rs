//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// Implements the OR operator according to the EVM specification.
pub struct Or;

impl<D> RuntimeFunction<D> for Or
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_or";

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
                .build_or(operand_1, operand_2, "OR")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for Or
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

/// Implements the XOR operator according to the EVM specification.
pub struct Xor;

impl<D> RuntimeFunction<D> for Xor
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_xor";

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
                .build_xor(operand_1, operand_2, "XOR")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for Xor
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

/// Implements the AND operator according to the EVM specification.
pub struct And;

impl<D> RuntimeFunction<D> for And
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_and";

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
                .build_and(operand_1, operand_2, "AND")
                .map(Into::into)?,
        ))
    }
}

impl<D> WriteLLVM<D> for And
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
/// Implements the SHL operator according to the EVM specification.
pub struct Shl;

impl<D> RuntimeFunction<D> for Shl
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_shl";

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
        let shift = Self::paramater(context, 0).into_int_value();
        let value = Self::paramater(context, 1).into_int_value();

        let overflow_block = context.append_basic_block("shift_left_overflow");
        let non_overflow_block = context.append_basic_block("shift_left_non_overflow");
        let join_block = context.append_basic_block("shift_left_join");

        let condition_is_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            shift,
            context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
            "shift_left_is_overflow",
        )?;
        context.build_conditional_branch(
            condition_is_overflow,
            overflow_block,
            non_overflow_block,
        )?;

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
        Ok(Some(result.as_basic_value()))
    }
}

impl<D> WriteLLVM<D> for Shl
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

/// Implements the SHR operator according to the EVM specification.
pub struct Shr;

impl<D> RuntimeFunction<D> for Shr
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_shr";

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
        let shift = Self::paramater(context, 0).into_int_value();
        let value = Self::paramater(context, 1).into_int_value();

        let overflow_block = context.append_basic_block("shift_right_overflow");
        let non_overflow_block = context.append_basic_block("shift_right_non_overflow");
        let join_block = context.append_basic_block("shift_right_join");

        let condition_is_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            shift,
            context.word_const((revive_common::BIT_LENGTH_WORD - 1) as u64),
            "shift_right_is_overflow",
        )?;
        context.build_conditional_branch(
            condition_is_overflow,
            overflow_block,
            non_overflow_block,
        )?;

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
        Ok(Some(result.as_basic_value()))
    }
}

impl<D> WriteLLVM<D> for Shr
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

/// Implements the SAR operator according to the EVM specification.
pub struct Sar;

impl<D> RuntimeFunction<D> for Sar
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_sar";

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
        let shift = Self::paramater(context, 0).into_int_value();
        let value = Self::paramater(context, 1).into_int_value();

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
        context.build_conditional_branch(
            condition_is_overflow,
            overflow_block,
            non_overflow_block,
        )?;

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
        Ok(Some(result.as_basic_value()))
    }
}

impl<D> WriteLLVM<D> for Sar
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
/// Implements the BYTE operator according to the EVM specification.
pub struct Byte;

impl<D> RuntimeFunction<D> for Byte
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_byte";

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
        let mask_byte_word = context.builder().build_int_z_extend(
            mask_byte,
            context.word_type(),
            "mask_byte_word",
        )?;

        let index_truncated = context.builder().build_int_truncate(
            operand_1,
            context.byte_type(),
            "index_truncated",
        )?;
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
        let byte =
            context
                .builder()
                .build_right_shift(masked_value, index_extended, false, "byte")?;

        Ok(Some(byte.as_basic_value_enum()))
    }
}

impl<D> WriteLLVM<D> for Byte
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
