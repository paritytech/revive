//! Translates a contract call.

use inkwell::values::BasicValue;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::PolkaVMCallReentrancyProtector;

const STATIC_CALL_FLAG: u32 = 0b0001_0000;
const REENTRANT_CALL_FLAG: u32 = 0b0000_1000;

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
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;
    let output_length = context.safe_truncate_int_to_xlen(output_length)?;

    let deposit_limit_pointer =
        context.build_alloca_at_entry(context.word_type(), "deposit_pointer");
    let flags = if static_call {
        let flags = REENTRANT_CALL_FLAG | STATIC_CALL_FLAG;
        context.build_store(deposit_limit_pointer, context.word_type().const_zero())?;
        context.xlen_type().const_int(flags as u64, false)
    } else {
        call_reentrancy_heuristic(
            context,
            deposit_limit_pointer.value,
            gas,
            input_length,
            output_length,
        )?
    };

    let address_pointer = context.build_address_argument_store(address)?;

    let value = value.unwrap_or_else(|| context.word_const(0));
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");
    context.build_store(value_pointer, value)?;

    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let output_offset = context.safe_truncate_int_to_xlen(output_offset)?;

    let input_pointer = context.build_heap_gep(input_offset, input_length)?;
    let output_pointer = context.build_heap_gep(output_offset, output_length)?;

    let output_length_pointer = context.build_alloca_at_entry(context.xlen_type(), "output_length");
    context.build_store(output_length_pointer, output_length)?;

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
        deposit_limit_pointer.to_int(context),
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

fn call_reentrancy_heuristic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    deposit_limit_pointer: inkwell::values::PointerValue<'ctx>,
    gas: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::IntValue<'ctx>>
where
    D: Dependency + Clone,
{
    let name = <PolkaVMCallReentrancyProtector as RuntimeFunction<D>>::NAME;
    let declaration = <PolkaVMCallReentrancyProtector as RuntimeFunction<D>>::declaration(context);
    let arguments = &[
        deposit_limit_pointer.into(),
        gas.into(),
        context
            .builder()
            .build_or(input_length, output_length, "input_length_or_output_length")?
            .into(),
    ];
    Ok(context
        .build_call(declaration, arguments, "call_flags")
        .unwrap_or_else(|| panic!("revive runtime function {name} should return a value"))
        .into_int_value())
}
