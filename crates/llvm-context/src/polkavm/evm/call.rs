//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

const STATIC_CALL_FLAG: u32 = 0b0001_0000;
const REENTRANT_CALL_FLAG: u32 = 0b0000_1000;
const SOLIDITY_TRANSFER_GAS_STIPEND_THRESHOLD: u64 = 2300;

/// Translates a contract call.
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

    let (flags, deposit_limit_value) = if static_call {
        let flags = REENTRANT_CALL_FLAG | STATIC_CALL_FLAG;
        (
            context.xlen_type().const_int(flags as u64, false),
            context.word_type().const_all_ones(),
        )
    } else {
        call_reentrancy_heuristic(context, gas, input_length, output_length)?
    };

    let deposit_pointer = context.build_alloca_at_entry(context.word_type(), "deposit_pointer");
    context.build_store(deposit_pointer, deposit_limit_value)?;

    let flags_and_callee = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        flags,
        address_pointer.to_int(context),
        "address_and_callee",
    )?;
    let deposit_and_value = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        deposit_pointer.to_int(context),
        value_pointer.to_int(context),
        "deposit_and_value",
    )?;
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
                flags_and_callee.into(),
                context.register_type().const_all_ones().into(),
                context.register_type().const_all_ones().into(),
                deposit_and_value.into(),
                input_data.into(),
                output_data.into(),
            ],
        )
        .unwrap_or_else(|| panic!("{name} should return a value"))
        .into_int_value();

    let is_success = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        success,
        context.integer_const(revive_common::BIT_LENGTH_X64, 0),
        "is_success",
    )?;

    Ok(context
        .builder()
        .build_int_z_extend(is_success, context.word_type(), "success")?
        .as_basic_value_enum())
}

#[allow(clippy::too_many_arguments)]
pub fn delegate_call<'ctx, D>(
    context: &mut Context<'ctx, D>,
    _gas: inkwell::values::IntValue<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
    _constants: Vec<Option<num::BigUint>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address_pointer = context.build_address_argument_store(address)?;

    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let output_offset = context.safe_truncate_int_to_xlen(output_offset)?;
    let output_length = context.safe_truncate_int_to_xlen(output_length)?;

    let input_pointer = context.build_heap_gep(input_offset, input_length)?;
    let output_pointer = context.build_heap_gep(output_offset, output_length)?;

    let output_length_pointer = context.build_alloca_at_entry(context.xlen_type(), "output_length");
    context.build_store(output_length_pointer, output_length)?;

    let deposit_pointer = context.build_alloca_at_entry(context.word_type(), "deposit_pointer");
    context.build_store(deposit_pointer, context.word_type().const_all_ones())?;

    let flags = context.xlen_type().const_int(0u64, false);

    let flags_and_callee = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        flags,
        address_pointer.to_int(context),
        "address_and_callee",
    )?;
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
                flags_and_callee.into(),
                context.register_type().const_all_ones().into(),
                context.register_type().const_all_ones().into(),
                deposit_pointer.to_int(context).into(),
                input_data.into(),
                output_data.into(),
            ],
        )
        .unwrap_or_else(|| panic!("{name} should return a value"))
        .into_int_value();

    let is_success = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        success,
        context.integer_const(revive_common::BIT_LENGTH_X64, 0),
        "is_success",
    )?;

    Ok(context
        .builder()
        .build_int_z_extend(is_success, context.word_type(), "success")?
        .as_basic_value_enum())
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

/// The Solidity `address.transfer`` and `address.send`` call detection heuristic.
///
/// # Why
/// This heuristic is an additional security feature to guard against re-entrancy attacks
/// in case contract authors violate Solidity best practices and use `address.transfer` or
/// `address.send`.
/// While contract authors are supposed to never use `address.transfer` or `address.send`,
/// for a small cost we can be extra defensive about it.
///
/// # How
/// The gas stipend emitted by solc for `transfer` and `send` is not static, thus:
/// - Dynamically allow re-entrancy only for calls considered not transfer or send.
/// - Detected balance transfers will supply 0 deposit limit instead of `u256::MAX`.
///
/// Calls are considered transfer or send if:
/// - (Input length | Output lenght) == 0;
/// - Gas <= 2300;
///
/// # Returns
/// The call flags xlen `IntValue` and the deposit limit word `IntValue`.
fn call_reentrancy_heuristic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    gas: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<(
    inkwell::values::IntValue<'ctx>,
    inkwell::values::IntValue<'ctx>,
)>
where
    D: Dependency + Clone,
{
    // Branch-free SSA implementation: First derive the heuristic boolean (int1) value.
    let input_length_or_output_length =
        context
            .builder()
            .build_or(input_length, output_length, "input_length_or_output_length")?;
    let is_no_input_no_output = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        context.xlen_type().const_zero(),
        input_length_or_output_length,
        "is_no_input_no_output",
    )?;
    let gas_stipend = context
        .word_type()
        .const_int(SOLIDITY_TRANSFER_GAS_STIPEND_THRESHOLD, false);
    let is_gas_stipend_for_transfer_or_send = context.builder().build_int_compare(
        inkwell::IntPredicate::ULE,
        gas,
        gas_stipend,
        "is_gas_stipend_for_transfer_or_send",
    )?;
    let is_balance_transfer = context.builder().build_and(
        is_no_input_no_output,
        is_gas_stipend_for_transfer_or_send,
        "is_balance_transfer",
    )?;
    let is_regular_call = context
        .builder()
        .build_not(is_balance_transfer, "is_balance_transfer_inverted")?;

    // Call flag: Left shift the heuristic boolean value.
    let is_regular_call_xlen = context.builder().build_int_z_extend(
        is_regular_call,
        context.xlen_type(),
        "is_balance_transfer_xlen",
    )?;
    let call_flags = context.builder().build_left_shift(
        is_regular_call_xlen,
        context.xlen_type().const_int(3, false),
        "flags",
    )?;

    // Deposit limit value: Sign-extended the heuristic boolean value.
    let deposit_limit_value = context.builder().build_int_s_extend(
        is_regular_call,
        context.word_type(),
        "deposit_limit_value",
    )?;

    Ok((call_flags, deposit_limit_value))
}
