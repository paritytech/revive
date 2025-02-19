//! The EVM linear memory pointer functions.
use inkwell::values::BasicValue;

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
        Attribute::MinSize,
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.xlen_type().into()],
            false,
        )
    }

    fn emit_body(&self, context: &Context<D>) -> anyhow::Result<()> {
        context.build_return(None);

        Ok(())
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

/// The heap GEP method.
pub struct GetElementPointer;

impl<D> RuntimeFunction<D> for GetElementPointer
where
    D: Dependency + Clone,
{
    const FUNCTION_NAME: &'static str = "__revive_heap_gep";

    const FUNCTION_ATTRIBUTES: &'static [Attribute] = &[
        Attribute::MinSize,
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.xlen_type().into()],
            false,
        )
    }

    fn emit_body(&self, context: &Context<D>) -> anyhow::Result<()> {
        let input_offset = Self::paramater(context, 0).into_int_value();
        let input_length = Self::paramater(context, 1).into_int_value();
        let input_pointer = context.builder().build_ptr_to_int(
            context.build_heap_gep(input_offset, input_length)?.value,
            context.xlen_type(),
            "event_input_offset",
        )?;

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::DEPOSIT_EVENT,
            &[
                context.xlen_type().const_zero().as_basic_value_enum(),
                context.xlen_type().const_zero().as_basic_value_enum(),
                input_pointer.as_basic_value_enum(),
                input_length.as_basic_value_enum(),
            ],
        );

        context.build_return(None);

        Ok(())
    }
}

impl<D> WriteLLVM<D> for GetElementPointer
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
