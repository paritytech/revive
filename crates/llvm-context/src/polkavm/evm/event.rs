//! Translates a log or event call.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

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

    if topics.is_empty() {
        let _ = context.build_runtime_call(
            runtime_api::DEPOSIT_EVENT,
            &[
                context.xlen_type().const_zero().as_basic_value_enum(),
                context.xlen_type().const_zero().as_basic_value_enum(),
                input_pointer.as_basic_value_enum(),
                input_length.as_basic_value_enum(),
            ],
        );
        return Ok(());
    }

    let name = match topics.len() {
        1 => "__log_1",
        2 => "__log_2",
        3 => "__log_3",
        4 => "__log_4",
        _ => unreachable!(),
    };
    let function = context.module().get_function(name).unwrap_or_else(|| {
        let position = context.basic_block();

        let mut parameters = vec![context.xlen_type().into(), context.xlen_type().into()];
        parameters.extend_from_slice(
            &topics
                .iter()
                .map(|_| context.word_type().into())
                .collect::<Vec<_>>(),
        );
        let function = context.module().add_function(
            name,
            context.void_type().fn_type(&parameters, false),
            None,
        );
        let block_entry = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(block_entry);

        let parameters = function.get_param_iter().collect::<Vec<_>>();
        let topics = &parameters[2..];
        let input_offset = parameters.first().unwrap();
        let input_length = parameters.get(1).unwrap();
        let topics_buffer_size = topics.len() * revive_common::BYTE_LENGTH_WORD;
        let topics_buffer_pointer = context.build_alloca(
            context.byte_type().array_type(topics_buffer_size as u32),
            "topics_buffer",
        );
        for (n, topic) in topics.iter().enumerate() {
            let topic_buffer_offset = context
                .xlen_type()
                .const_int((n * revive_common::BYTE_LENGTH_WORD) as u64, false);
            context
                .build_store(
                    context.build_gep(
                        topics_buffer_pointer,
                        &[context.xlen_type().const_zero(), topic_buffer_offset],
                        context.byte_type(),
                        "topic_buffer_gep",
                    ),
                    context
                        .build_byte_swap(topic.as_basic_value_enum())
                        .unwrap(),
                )
                .unwrap();
        }
        let arguments = [
            context
                .builder()
                .build_ptr_to_int(
                    topics_buffer_pointer.value,
                    context.xlen_type(),
                    "event_topics_offset",
                )
                .unwrap()
                .as_basic_value_enum(),
            context
                .xlen_type()
                .const_int(topics_buffer_size as u64, false)
                .as_basic_value_enum(),
            input_offset.as_basic_value_enum(),
            input_length.as_basic_value_enum(),
        ];

        let _ = context.build_runtime_call(runtime_api::DEPOSIT_EVENT, &arguments);
        context.builder().build_return(None).unwrap();

        context.set_basic_block(position);

        function
    });

    let mut arguments = vec![
        input_pointer.as_basic_value_enum().into(),
        input_length.as_basic_value_enum().into(),
    ];
    arguments.extend_from_slice(
        &topics
            .iter()
            .map(|value| value.as_basic_value_enum().into())
            .collect::<Vec<_>>(),
    );
    let _ = context
        .builder()
        .build_direct_call(function, &arguments[..], "call_log");

    Ok(())
}
