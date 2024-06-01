//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

static STATIC_CALL_FLAG: u32 = 0b0001_0000;

/// Translates a contract call.
///
/// If the `simulation_address` is specified, the call is
/// substituted with another instruction according to the specification.
#[allow(clippy::too_many_arguments)]
pub fn call<'ctx, D>(
    context: &mut Context<'ctx, D>,
    gas: inkwell::values::IntValue<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    value: Option<inkwell::values::IntValue<'ctx>>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
    static_call: bool,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address_pointer = context.build_alloca(context.word_type(), "address_ptr");
    context.build_store(address_pointer, address)?;

    let value_pointer = if let Some(value) = value {
        let value_pointer = context.build_alloca(context.value_type(), "value");
        context.build_store(value_pointer, value)?;
        value_pointer.value
    } else {
        context.sentinel_pointer()
    };

    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let output_offset = context.safe_truncate_int_to_xlen(output_offset)?;
    let output_length = context.safe_truncate_int_to_xlen(output_length)?;

    let gas = context
        .builder()
        .build_int_truncate(gas, context.integer_type(64), "gas")?;

    let flags = if static_call { STATIC_CALL_FLAG } else { 0 };

    let input_pointer = context.build_heap_gep(input_offset, input_length)?;
    let output_pointer = context.build_heap_gep(output_offset, output_length)?;

    let output_length_pointer = context.get_global(crate::polkavm::GLOBAL_RETURN_DATA_SIZE)?;
    context.build_store(output_length_pointer.into(), output_length)?;

    let argument_pointer = pallet_contracts_pvm_llapi::calling_convention::Spill::new(
        context.builder(),
        pallet_contracts_pvm_llapi::calling_convention::call(context.llvm()),
        "call_arguments",
    )?
    .next(context.xlen_type().const_int(flags as u64, false))?
    .next(address_pointer.value)?
    .next(gas)?
    .skip()
    .next(context.sentinel_pointer())?
    .next(value_pointer)?
    .next(input_pointer.value)?
    .next(input_length)?
    .next(output_pointer.value)?
    .next(output_length_pointer.value)?
    .done();

    let name = runtime_api::imports::CALL;
    let arguments = context.builder().build_ptr_to_int(
        argument_pointer,
        context.xlen_type(),
        "argument_pointer",
    )?;
    let success = context
        .build_runtime_call(name, &[arguments.into()])
        .unwrap_or_else(|| panic!("{name} should return a value"))
        .into_int_value();

    let is_success = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        success,
        context.xlen_type().const_zero(),
        "is_success",
    )?;

    Ok(context
        .builder()
        .build_int_z_extend(is_success, context.word_type(), "success")?
        .as_basic_value_enum())
}

#[allow(clippy::too_many_arguments)]
pub fn delegate_call<'ctx, D>(
    _context: &mut Context<'ctx, D>,
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
    todo!()
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
