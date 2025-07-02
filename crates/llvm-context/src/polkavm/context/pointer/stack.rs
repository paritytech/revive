//! The revive simulated EVM stack variable functions.

use inkwell::values::BasicValueEnum;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// Load a word size value from a heap pointer.
pub struct DeclareVariable;

impl<D> RuntimeFunction<D> for DeclareVariable
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_declare_variable";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context
            .llvm
            .ptr_type(Default::default())
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;
        let value = context
            .builder()
            .build_load(context.word_type(), pointer.value, "value")?;
        context
            .basic_block()
            .get_last_instruction()
            .expect("Always exists")
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");

        let swapped_value = context.build_byte_swap(value)?;
        Ok(Some(swapped_value))
    }
}

impl<D> WriteLLVM<D> for LoadWord
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

/// Store a word size value through a heap pointer.
pub struct StoreWord;

impl<D> RuntimeFunction<D> for StoreWord
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_store_heap_word";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;

        let value = context.build_byte_swap(Self::paramater(context, 1))?;

        context
            .builder()
            .build_store(pointer.value, value)?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        Ok(None)
    }
}

impl<D> WriteLLVM<D> for StoreWord
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
