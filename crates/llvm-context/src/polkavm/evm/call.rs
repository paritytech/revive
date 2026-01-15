//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;

const STATIC_CALL_FLAG: u64 = 0b0001_0000;
const REENTRANT_CALL_FLAG: u64 = 0b0000_1000;

/// Translates a contract call.
pub fn call<'ctx>(
    context: &mut Context<'ctx>,
    gas: inkwell::values::IntValue<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    value: Option<inkwell::values::IntValue<'ctx>>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
    static_call: bool,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let address_pointer = context.build_address_argument_store(address)?;

    let value = value.unwrap_or_else(|| context.word_const(0));
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");
    context.build_store(value_pointer, value)?;

    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let output_offset = context.safe_truncate_int_to_xlen(output_offset)?;
    let output_length = context.safe_truncate_int_to_xlen(output_length)?;

    let input_pointer = context.build_heap_gep(input_offset, input_length)?;
    let output_pointer = context.build_heap_gep(output_offset, output_length)?;

    let output_length_pointer = context.build_alloca_at_entry(context.xlen_type(), "output_length");
    context.build_store(output_length_pointer, output_length)?;

    let flags = if static_call {
        REENTRANT_CALL_FLAG | STATIC_CALL_FLAG
    } else {
        REENTRANT_CALL_FLAG
    };

    let input_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        input_length,
        input_pointer.to_int(context),
        "input_data",
    )?;
    let output_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        output_length_pointer.to_int(context),
        output_pointer.to_int(context),
        "output_data",
    )?;

    let name = revive_runtime_api::polkavm_imports::CALL;
    let success = context
        .build_runtime_call(
            name,
            &[
                context.xlen_type().const_int(flags, false).into(),
                address_pointer.to_int(context).into(),
                value_pointer.to_int(context).into(),
                clip_call_gas(context, gas)?,
                input_data.into(),
                output_data.into(),
            ],
        )
        .unwrap_or_else(|| panic!("{name} should return a value"))
        .into_int_value();

    let is_success = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        success,
        context.integer_const(revive_common::BIT_LENGTH_X32, 0),
        "is_success",
    )?;

    Ok(context
        .builder()
        .build_int_z_extend(is_success, context.word_type(), "success")?
        .as_basic_value_enum())
}

pub fn delegate_call<'ctx>(
    context: &mut Context<'ctx>,
    gas: inkwell::values::IntValue<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let address_pointer = context.build_address_argument_store(address)?;

    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let output_offset = context.safe_truncate_int_to_xlen(output_offset)?;
    let output_length = context.safe_truncate_int_to_xlen(output_length)?;

    let input_pointer = context.build_heap_gep(input_offset, input_length)?;
    let output_pointer = context.build_heap_gep(output_offset, output_length)?;

    let output_length_pointer = context.build_alloca_at_entry(context.xlen_type(), "output_length");
    context.build_store(output_length_pointer, output_length)?;

    let input_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        input_length,
        input_pointer.to_int(context),
        "input_data",
    )?;
    let output_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        output_length_pointer.to_int(context),
        output_pointer.to_int(context),
        "output_data",
    )?;

    let name = revive_runtime_api::polkavm_imports::DELEGATE_CALL;
    let success = context
        .build_runtime_call(
            name,
            &[
                context.xlen_type().const_int(0u64, false).into(),
                address_pointer.to_int(context).into(),
                clip_call_gas(context, gas)?,
                input_data.into(),
                output_data.into(),
            ],
        )
        .unwrap_or_else(|| panic!("{name} should return a value"))
        .into_int_value();

    let is_success = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        success,
        context.integer_const(revive_common::BIT_LENGTH_X32, 0),
        "is_success",
    )?;

    Ok(context
        .builder()
        .build_int_z_extend(is_success, context.word_type(), "success")?
        .as_basic_value_enum())
}

/// Translates the Yul `linkersymbol` instruction.
pub fn linker_symbol<'ctx>(
    context: &mut Context<'ctx>,
    path: &str,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    context.declare_global(
        path,
        context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS),
        Default::default(),
    );
    let address = context.build_load(context.get_global(path)?.into(), "linker_symbol")?;
    Ok(context
        .builder()
        .build_int_z_extend(
            address.into_int_value(),
            context.word_type(),
            "linker_symbol_zext",
        )?
        .into())
}

/// The runtime implements gas as `u64` so we clip the stipend to `u64::MAX`.
fn clip_call_gas<'ctx>(
    context: &Context<'ctx>,
    gas: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let builder = context.builder();

    let clipped = context.register_type().const_all_ones();
    let is_overflow = builder.build_int_compare(
        inkwell::IntPredicate::UGT,
        gas,
        builder.build_int_z_extend(clipped, context.word_type(), "gas_clipped")?,
        "is_gas_overflow",
    )?;
    let truncated = builder.build_int_truncate(gas, context.register_type(), "gas_truncated")?;
    let call_gas = builder.build_select(is_overflow, clipped, truncated, "call_gas")?;

    Ok(call_gas)
}
