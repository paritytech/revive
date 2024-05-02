//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates a contract call.
/// If the `simulation_address` is specified, the call is substituted with another instruction
/// according to the specification.
#[allow(clippy::too_many_arguments)]
pub fn default<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _function: FunctionDeclaration<'ctx>,
    _gas: inkwell::values::IntValue<'ctx>,
    _address: inkwell::values::IntValue<'ctx>,
    _value: Option<inkwell::values::IntValue<'ctx>>,
    _input_offset: inkwell::values::IntValue<'ctx>,
    _input_length: inkwell::values::IntValue<'ctx>,
    _output_offset: inkwell::values::IntValue<'ctx>,
    _output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!();

    /*
    let ordinary_block = context.append_basic_block("contract_call_ordinary_block");
    let join_block = context.append_basic_block("contract_call_join_block");

    let result_pointer = context.build_alloca(context.field_type(), "contract_call_result_pointer");
    context.build_store(result_pointer, context.field_const(0));

    context.builder().build_switch(
        address,
        ordinary_block,
        &[(
            context.field_const(zkevm_opcode_defs::ADDRESS_IDENTITY.into()),
            identity_block,
        )],
    )?;

    {
        context.set_basic_block(identity_block);
        let result = identity(context, output_offset, input_offset, output_length)?;
        context.build_store(result_pointer, result);
        context.build_unconditional_branch(join_block);
    }

    context.set_basic_block(ordinary_block);
    let result = if let Some(value) = value {
        default_wrapped(
            context,
            function,
            gas,
            value,
            address,
            input_offset,
            input_length,
            output_offset,
            output_length,
        )?
    } else {
        let function = Runtime::default_call(context, function);
        context
            .build_call(
                function,
                &[
                    gas.as_basic_value_enum(),
                    address.as_basic_value_enum(),
                    input_offset.as_basic_value_enum(),
                    input_length.as_basic_value_enum(),
                    output_offset.as_basic_value_enum(),
                    output_length.as_basic_value_enum(),
                ],
                "default_call",
            )
            .expect("Always exists")
    };
    context.build_store(result_pointer, result);
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    let result = context.build_load(result_pointer, "contract_call_result");
    Ok(result)
    */
}

/// Translates the Yul `linkersymbol` instruction.
pub fn linker_symbol<'ctx, D>(
    context: &mut Context<'ctx, D>,
    mut arguments: [Argument<'ctx>; 1],
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let path = arguments[0]
        .original
        .take()
        .ok_or_else(|| anyhow::anyhow!("Linker symbol literal is missing"))?;

    Ok(context
        .resolve_library(path.as_str())?
        .as_basic_value_enum())
}
