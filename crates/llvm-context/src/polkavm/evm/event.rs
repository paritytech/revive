//! Translates a log or event call.

use inkwell::values::BasicValue;

use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

const LOG_FUNCTION_ATTRIBUTES: &[Attribute] = &[
    Attribute::MinSize,
    Attribute::NoFree,
    Attribute::NoRecurse,
    Attribute::WillReturn,
];

/// A function for emitting EVM event logs from contract code.
pub struct EventLog<const N: usize>;

impl<D> RuntimeFunction<D> for EventLog<0>
where
    D: Dependency + Clone,
{
    const FUNCTION_NAME: &'static str = "__revive_runtime_log_0";

    const FUNCTION_ATTRIBUTES: &'static [Attribute] = LOG_FUNCTION_ATTRIBUTES;

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

/// Translates a log or event call.
pub fn log<'ctx, D>(
    context: &mut Context<'ctx, D>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    topics: Vec<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let input_pointer = context.builder().build_ptr_to_int(
        context.build_heap_gep(input_offset, input_length)?.value,
        context.xlen_type(),
        "event_input_offset",
    )?;

    let arguments = if topics.is_empty() {
        [
            context.xlen_type().const_zero().as_basic_value_enum(),
            context.xlen_type().const_zero().as_basic_value_enum(),
            input_pointer.as_basic_value_enum(),
            input_length.as_basic_value_enum(),
        ]
    } else {
        let topics_buffer_size = topics.len() * revive_common::BYTE_LENGTH_WORD;
        let topics_buffer_pointer = context.build_alloca(
            context.byte_type().array_type(topics_buffer_size as u32),
            "topics_buffer",
        );

        for (n, topic) in topics.iter().enumerate() {
            let topic_buffer_offset = context
                .xlen_type()
                .const_int((n * revive_common::BYTE_LENGTH_WORD) as u64, false);
            context.build_store(
                context.build_gep(
                    topics_buffer_pointer,
                    &[context.xlen_type().const_zero(), topic_buffer_offset],
                    context.byte_type(),
                    "topic_buffer_gep",
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
                .const_int(topics.len() as u64, false)
                .as_basic_value_enum(),
            input_pointer.as_basic_value_enum(),
            input_length.as_basic_value_enum(),
        ]
    };

    let _ = context.build_runtime_call(
        revive_runtime_api::polkavm_imports::DEPOSIT_EVENT,
        &arguments,
    );

    Ok(())
}
