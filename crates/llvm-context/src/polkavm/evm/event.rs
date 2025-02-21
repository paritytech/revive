//! Translates a log or event call.

use inkwell::values::BasicValue;

use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// A function for emitting EVM event logs from contract code.
pub struct EventLog<const N: usize>;

impl<D, const N: usize> RuntimeFunction<D> for EventLog<N>
where
    D: Dependency + Clone,
{
    const NAME: &'static str = match N {
        0 => "__revive_log_0",
        1 => "__revive_log_1",
        2 => "__revive_log_2",
        3 => "__revive_log_3",
        4 => "__revive_log_4",
        _ => unreachable!(),
    };

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        let mut parameter_types = vec![context.xlen_type().into(), context.xlen_type().into()];
        parameter_types.extend_from_slice(&[context.word_type().into(); N]);
        context.void_type().fn_type(&parameter_types, false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let input_offset = Self::paramater(context, 0).into_int_value();
        let input_length = Self::paramater(context, 1).into_int_value();
        let input_pointer = context.builder().build_ptr_to_int(
            context.build_heap_gep(input_offset, input_length)?.value,
            context.xlen_type(),
            "event_input_offset",
        )?;

        let arguments = if N == 0 {
            [
                context.xlen_type().const_zero().as_basic_value_enum(),
                context.xlen_type().const_zero().as_basic_value_enum(),
                input_pointer.as_basic_value_enum(),
                input_length.as_basic_value_enum(),
            ]
        } else {
            let topics_buffer_size = N * revive_common::BYTE_LENGTH_WORD;
            let topics_buffer_pointer = context.build_alloca_at_entry(
                context.byte_type().array_type(topics_buffer_size as u32),
                "topics_buffer",
            );

            for n in 0..N {
                let topic = Self::paramater(context, n as u32 + 2);
                let topic_buffer_offset = context
                    .xlen_type()
                    .const_int((n * revive_common::BYTE_LENGTH_WORD) as u64, false);
                context.build_store(
                    context.build_gep(
                        topics_buffer_pointer,
                        &[context.xlen_type().const_zero(), topic_buffer_offset],
                        context.byte_type(),
                        &format!("topic_buffer_{N}_gep"),
                    ),
                    context.build_byte_swap(topic.as_basic_value_enum())?,
                )?;
            }

            [
                context
                    .builder()
                    .build_ptr_to_int(
                        topics_buffer_pointer.value,
                        context.xlen_type(),
                        "event_topics_offset",
                    )?
                    .as_basic_value_enum(),
                context
                    .xlen_type()
                    .const_int(N as u64, false)
                    .as_basic_value_enum(),
                input_pointer.as_basic_value_enum(),
                input_length.as_basic_value_enum(),
            ]
        };

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::DEPOSIT_EVENT,
            &arguments,
        );

        Ok(None)
    }
}

impl<D> WriteLLVM<D> for EventLog<0>
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

impl<D> WriteLLVM<D> for EventLog<1>
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

impl<D> WriteLLVM<D> for EventLog<2>
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

impl<D> WriteLLVM<D> for EventLog<3>
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

impl<D> WriteLLVM<D> for EventLog<4>
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

/// Translates a log or event call.
pub fn log<'ctx, D, const N: usize>(
    context: &mut Context<'ctx, D>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    topics: [inkwell::values::BasicValueEnum<'ctx>; N],
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let declaration = <EventLog<N> as RuntimeFunction<D>>::declaration(context);
    let mut arguments = vec![
        context.safe_truncate_int_to_xlen(input_offset)?.into(),
        context.safe_truncate_int_to_xlen(input_length)?.into(),
    ];
    arguments.extend_from_slice(&topics);
    context.build_call(declaration, &arguments, "log");
    Ok(())
}
