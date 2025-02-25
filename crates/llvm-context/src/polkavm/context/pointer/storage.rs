//! The revive storage pointer functions.

use inkwell::values::BasicValueEnum;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// Load a word size value from a storage pointer.
pub struct LoadWord;

impl<D> RuntimeFunction<D> for LoadWord
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_load_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.xlen_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let is_transient = Self::paramater(context, 0);
        let key_value = Self::paramater(context, 1);

        let key_pointer = context.build_alloca_at_entry(context.word_type(), "key_pointer");
        let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");
        let length_pointer = context.build_alloca_at_entry(context.xlen_type(), "length_pointer");

        context
            .builder()
            .build_store(key_pointer.value, key_value)?;
        context.build_store(value_pointer, context.word_const(0))?;
        context.build_store(
            length_pointer,
            context
                .xlen_type()
                .const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
        )?;

        let arguments = [
            is_transient,
            key_pointer.to_int(context).into(),
            context.xlen_type().const_all_ones().into(),
            value_pointer.to_int(context).into(),
            length_pointer.to_int(context).into(),
        ];
        context.build_runtime_call(revive_runtime_api::polkavm_imports::GET_STORAGE, &arguments);

        // We do not to check the return value: Solidity assumes infallible loads.
        // If a key doesn't exist the "zero" value is returned (ensured by above write).

        Ok(Some(context.build_load(value_pointer, "storage_value")?))
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

/// Store a word size value through a storage pointer.
pub struct StoreWord;

impl<D> RuntimeFunction<D> for StoreWord
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_store_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[
                context.xlen_type().into(),
                context.word_type().into(),
                context.word_type().into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let is_transient = Self::paramater(context, 0);
        let key = Self::paramater(context, 1);
        let value = Self::paramater(context, 2);

        let key_pointer = context.build_alloca_at_entry(context.word_type(), "key_pointer");
        let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");

        context.build_store(key_pointer, key)?;
        context.build_store(value_pointer, value)?;

        let arguments = [
            is_transient,
            key_pointer.to_int(context).into(),
            context.xlen_type().const_all_ones().into(),
            value_pointer.to_int(context).into(),
            context.integer_const(crate::polkavm::XLEN, 32).into(),
        ];
        context.build_runtime_call(revive_runtime_api::polkavm_imports::SET_STORAGE, &arguments);

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
