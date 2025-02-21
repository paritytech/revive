//! The revive compiler runtime functions.

use inkwell::values::BasicValue;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// Pointers are represented as opaque 256 bit integer values in EVM.
/// In practice, they should never exceed a register sized bit value.
/// However, we still protect against this possibility here: Heap index
/// offsets are generally untrusted and potentially represent valid
/// (but wrong) pointers when truncated.
pub struct WordToPointer;

impl<D> RuntimeFunction<D> for WordToPointer
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_int_truncate";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context
            .xlen_type()
            .fn_type(&[context.word_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let value = Self::paramater(context, 0).into_int_value();
        let truncated =
            context
                .builder()
                .build_int_truncate(value, context.xlen_type(), "offset_truncated")?;
        let extended = context.builder().build_int_z_extend(
            truncated,
            context.word_type(),
            "offset_extended",
        )?;
        let is_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            extended,
            "compare_truncated_extended",
        )?;

        let block_continue = context.append_basic_block("offset_pointer_ok");
        let block_trap = context.append_basic_block("offset_pointer_overflow");
        context.build_conditional_branch(is_overflow, block_trap, block_continue)?;

        context.set_basic_block(block_trap);
        context.build_call(context.intrinsics().trap, &[], "invalid_trap");
        context.build_unreachable();

        context.set_basic_block(block_continue);
        Ok(Some(truncated.as_basic_value_enum()))
    }
}

impl<D> WriteLLVM<D> for WordToPointer
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
