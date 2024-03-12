//!
//! Translates a log or event call.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates a log or event call.
///
/// The decoding logic is implemented in a system contract, which is called from here.
///
/// There are several cases of the translation for the sake of efficiency, since the front-end
/// emits topics and values sequentially by one, but the LLVM intrinsic and bytecode instruction
/// accept two at once.
///
pub fn log<'ctx, D>(
    context: &mut Context<'ctx, D>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    topics: Vec<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    /*
    let failure_block = context.append_basic_block("event_failure_block");
    let join_block = context.append_basic_block("event_join_block");

    let gas = crate::eravm::evm::ether_gas::gas(context)?.into_int_value();
    let abi_data = crate::eravm::utils::abi_data(
        context,
        input_offset,
        input_length,
        Some(gas),
        AddressSpace::Heap,
        true,
    )?;
    let mut extra_abi_data = Vec::with_capacity(1 + topics.len());
    extra_abi_data.push(context.field_const(topics.len() as u64));
    extra_abi_data.extend(topics);

    let result = context
        .build_call(
            context.llvm_runtime().far_call,
            crate::eravm::utils::external_call_arguments(
                context,
                abi_data.as_basic_value_enum(),
                context.field_const(zkevm_opcode_defs::ADDRESS_EVENT_WRITER as u64),
                extra_abi_data,
                None,
            )
            .as_slice(),
            "event_writer_call_external",
        )
        .expect("Always returns a value");

    let result_status_code_boolean = context
        .builder()
        .build_extract_value(
            result.into_struct_value(),
            1,
            "event_writer_external_result_status_code_boolean",
        )
        .expect("Always exists");
    context.build_conditional_branch(
        result_status_code_boolean.into_int_value(),
        join_block,
        failure_block,
    )?;

    context.set_basic_block(failure_block);
    crate::eravm::evm::r#return::revert(context, context.field_const(0), context.field_const(0))?;

    context.set_basic_block(join_block);
    */
    Ok(())
}
