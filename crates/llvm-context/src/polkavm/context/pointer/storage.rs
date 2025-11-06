//! The revive storage pointer functions.

use inkwell::values::BasicValueEnum;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Load a word size value from a storage pointer.
pub struct LoadWord;

impl RuntimeFunction for LoadWord {
    const NAME: &'static str = "__revive_load_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.llvm().ptr_type(Default::default()).into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        Ok(Some(emit_load(
            context,
            Self::paramater(context, 0),
            false,
        )?))
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

/// Load a word size value from a transient storage pointer.
pub struct LoadTransientWord;

impl RuntimeFunction for LoadTransientWord {
    const NAME: &'static str = "__revive_load_transient_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.llvm().ptr_type(Default::default()).into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        Ok(Some(emit_load(context, Self::paramater(context, 0), true)?))
    }
}

impl WriteLLVM for LoadTransientWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Store a word size value through a storage pointer.
pub struct StoreWord;

impl RuntimeFunction for StoreWord {
    const NAME: &'static str = "__revive_store_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[
                context.llvm().ptr_type(Default::default()).into(),
                context.llvm().ptr_type(Default::default()).into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        emit_store(
            context,
            Self::paramater(context, 0),
            Self::paramater(context, 1),
            false,
        )?;

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

/// Store a word size value through a transient storage pointer.
pub struct StoreTransientWord;

impl RuntimeFunction for StoreTransientWord {
    const NAME: &'static str = "__revive_store_transient_storage_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[
                context.llvm().ptr_type(Default::default()).into(),
                context.llvm().ptr_type(Default::default()).into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        emit_store(
            context,
            Self::paramater(context, 0),
            Self::paramater(context, 1),
            true,
        )?;

        Ok(None)
    }
}

impl WriteLLVM for StoreTransientWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

fn emit_load<'ctx>(
    context: &mut Context<'ctx>,
    key: BasicValueEnum<'ctx>,
    transient: bool,
) -> anyhow::Result<BasicValueEnum<'ctx>> {
    let is_transient = context.xlen_type().const_int(transient as u64, false);
    let key_pointer = context.build_alloca_at_entry(context.word_type(), "key_pointer");
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");

    let mut key = context.build_load(
        super::Pointer::new(
            context.word_type(),
            Default::default(),
            key.into_pointer_value(),
        ),
        "key",
    )?;
    if !transient {
        key = context.build_byte_swap(key)?;
    }
    context.builder().build_store(key_pointer.value, key)?;

    let arguments = [
        is_transient.into(),
        key_pointer.to_int(context).into(),
        value_pointer.to_int(context).into(),
    ];
    context.build_runtime_call(revive_runtime_api::polkavm_imports::GET_STORAGE, &arguments);

    // We do not to check the return value: Solidity assumes infallible loads.
    // If a key doesn't exist the syscall returns zero.

    let value = context.build_load(value_pointer, "storage_value")?;
    Ok(if transient {
        value
    } else {
        context.build_byte_swap(value)?
    })
}

fn emit_store<'ctx>(
    context: &mut Context<'ctx>,
    key: BasicValueEnum<'ctx>,
    value: BasicValueEnum<'ctx>,
    transient: bool,
) -> anyhow::Result<()> {
    let is_transient = context.xlen_type().const_int(transient as u64, false);
    let key_pointer = context.build_alloca_at_entry(context.word_type(), "key_pointer");
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");

    let mut key = context.build_load(
        super::Pointer::new(
            context.word_type(),
            Default::default(),
            key.into_pointer_value(),
        ),
        "key",
    )?;
    let mut value = context.build_load(
        super::Pointer::new(
            context.word_type(),
            Default::default(),
            value.into_pointer_value(),
        ),
        "value",
    )?;
    if !transient {
        key = context.build_byte_swap(key)?;
        value = context.build_byte_swap(value)?;
    }

    context.build_store(key_pointer, key)?;
    context.build_store(value_pointer, value)?;

    let arguments = [
        is_transient.into(),
        key_pointer.to_int(context).into(),
        value_pointer.to_int(context).into(),
    ];
    context.build_runtime_call(revive_runtime_api::polkavm_imports::SET_STORAGE, &arguments);

    Ok(())
}
