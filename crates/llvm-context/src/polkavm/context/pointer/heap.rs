//! The revive simulated EVM linear memory pointer functions.

use inkwell::values::BasicValueEnum;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Load a word size value from a heap pointer.
pub struct LoadWord;

impl RuntimeFunction for LoadWord {
    const NAME: &'static str = "__revive_load_heap_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
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

impl WriteLLVM for LoadWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Store a word size value through a heap pointer.
pub struct StoreWord;

impl RuntimeFunction for StoreWord {
    const NAME: &'static str = "__revive_store_heap_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
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

impl WriteLLVM for StoreWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}
