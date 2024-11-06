//! Translates a log or event call.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates a log or event call.
///
/// TODO: Splitting up into dedicated functions (log0..log4)
/// could potentially decrease code sizes (LLVM can still decide to inline).
/// However, passing i256 parameters is counter productive and
/// I've found that splitting it up actualy increases code size.
/// Should be reviewed after 64bit support.
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
