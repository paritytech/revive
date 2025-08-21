//! The revive compiler runtime functions.

use inkwell::values::BasicValue;

use crate::polkavm::context::function::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Pointers are represented as opaque 256 bit integer values in EVM.
/// In practice, they should never exceed a register sized bit value.
/// However, we still protect against this possibility here: Heap index
/// offsets are generally untrusted and potentially represent valid
/// (but wrong) pointers when truncated.
pub struct WordToPointer;

impl RuntimeFunction for WordToPointer {
    const NAME: &'static str = "__revive_int_truncate";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::WillReturn,
        Attribute::NoFree,
        Attribute::AlwaysInline,
    ];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .xlen_type()
            .fn_type(&[context.word_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
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

impl WriteLLVM for WordToPointer {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// The revive runtime exit function.
pub struct Exit;

impl RuntimeFunction for Exit {
    const NAME: &'static str = "__revive_exit";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoReturn,
        Attribute::NoFree,
        Attribute::AlwaysInline,
    ];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
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
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let flags = Self::paramater(context, 0).into_int_value();
        let offset = Self::paramater(context, 1).into_int_value();
        let length = Self::paramater(context, 2).into_int_value();

        let offset_truncated = context.safe_truncate_int_to_xlen(offset)?;
        let length_truncated = context.safe_truncate_int_to_xlen(length)?;
        let heap_pointer = context.build_heap_gep(offset_truncated, length_truncated)?;
        let offset_pointer = context.builder().build_ptr_to_int(
            heap_pointer.value,
            context.xlen_type(),
            "return_data_ptr_to_int",
        )?;

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::RETURN,
            &[flags.into(), offset_pointer.into(), length_truncated.into()],
        );
        context.build_unreachable();

        Ok(None)
    }
}

impl WriteLLVM for Exit {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}
