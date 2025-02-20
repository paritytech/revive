//! The EVM linear memory pointer functions.

use inkwell::values::BasicValueEnum;

use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// The heap pointer load method.
pub struct LoadPointer;

impl<D> RuntimeFunction<D> for LoadPointer
where
    D: Dependency + Clone,
{
    const FUNCTION_NAME: &'static str = "__revive_load_heap_pointer";

    const FUNCTION_ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.xlen_type().into(), context.xlen_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = Self::paramater(context, 1).into_int_value();
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

impl<D> WriteLLVM<D> for LoadPointer
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
