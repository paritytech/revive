//!
//! Translates a contract call.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::argument::Argument;
use crate::eravm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::eravm::context::function::runtime::Runtime;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Translates a contract call.
///
/// If the `simulation_address` is specified, the call is substituted with another instruction
/// according to the specification.
///
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
    if context.is_system_mode() {
        let simulation_address = constants
            .get_mut(1)
            .and_then(|option| option.take())
            .and_then(|value| value.to_u16());

        match simulation_address {
            Some(revive_common::ERAVM_ADDRESS_TO_L1) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "to_l1",
                )?;

                let is_first = gas;
                let in_0 = value.expect("Always exists");
                let in_1 = input_offset;

                return crate::eravm::extensions::general::to_l1(context, is_first, in_0, in_1);
            }
            Some(revive_common::ERAVM_ADDRESS_CODE_ADDRESS) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "code_address",
                )?;

                return crate::eravm::extensions::general::code_source(context);
            }
            Some(revive_common::ERAVM_ADDRESS_PRECOMPILE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "precompile",
                )?;

                let in_0 = gas;
                let gas_left = input_offset;

                return crate::eravm::extensions::general::precompile(context, in_0, gas_left);
            }
            Some(revive_common::ERAVM_ADDRESS_META) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "meta",
                )?;

                return crate::eravm::extensions::general::meta(context);
            }
            Some(revive_common::ERAVM_ADDRESS_MIMIC_CALL) => {
                let address = gas;
                let abi_data = input_offset;
                let mimic = input_length;

                return crate::eravm::extensions::call::mimic(
                    context,
                    context.llvm_runtime().mimic_call,
                    address,
                    mimic,
                    abi_data.as_basic_value_enum(),
                    vec![],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_SYSTEM_MIMIC_CALL) => {
                let address = gas;
                let abi_data = input_offset;
                let mimic = input_length;
                let extra_value_1 = output_offset;
                let extra_value_2 = output_length;

                return crate::eravm::extensions::call::mimic(
                    context,
                    context.llvm_runtime().mimic_call,
                    address,
                    mimic,
                    abi_data.as_basic_value_enum(),
                    vec![extra_value_1, extra_value_2],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_MIMIC_CALL_BYREF) => {
                let address = gas;
                let mimic = input_length;
                let abi_data = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;

                return crate::eravm::extensions::call::mimic(
                    context,
                    context.llvm_runtime().mimic_call_byref,
                    address,
                    mimic,
                    abi_data.as_basic_value_enum(),
                    vec![],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_SYSTEM_MIMIC_CALL_BYREF) => {
                let address = gas;
                let mimic = input_length;
                let abi_data = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
                let extra_value_1 = output_offset;
                let extra_value_2 = output_length;

                return crate::eravm::extensions::call::mimic(
                    context,
                    context.llvm_runtime().mimic_call_byref,
                    address,
                    mimic,
                    abi_data,
                    vec![extra_value_1, extra_value_2],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_RAW_FAR_CALL) => {
                let address = gas;
                let abi_data = input_length;

                return crate::eravm::extensions::call::raw_far(
                    context,
                    context.llvm_runtime().modify(function, false)?,
                    address,
                    abi_data.as_basic_value_enum(),
                    output_offset,
                    output_length,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_RAW_FAR_CALL_BYREF) => {
                let address = gas;
                let abi_data = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;

                return crate::eravm::extensions::call::raw_far(
                    context,
                    context.llvm_runtime().modify(function, true)?,
                    address,
                    abi_data,
                    output_offset,
                    output_length,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_SYSTEM_CALL) => {
                let address = gas;
                let abi_data = input_length;
                let extra_value_1 = value.expect("Always exists");
                let extra_value_2 = input_offset;
                let extra_value_3 = output_offset;
                let extra_value_4 = output_length;

                return crate::eravm::extensions::call::system(
                    context,
                    context.llvm_runtime().modify(function, false)?,
                    address,
                    abi_data.as_basic_value_enum(),
                    context.field_const(0),
                    context.field_const(0),
                    vec![extra_value_1, extra_value_2, extra_value_3, extra_value_4],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_SYSTEM_CALL_BYREF) => {
                let address = gas;
                let abi_data = context.get_global_value(crate::eravm::GLOBAL_ACTIVE_POINTER)?;
                let extra_value_1 = value.expect("Always exists");
                let extra_value_2 = input_offset;
                let extra_value_3 = output_offset;
                let extra_value_4 = output_length;

                return crate::eravm::extensions::call::system(
                    context,
                    context.llvm_runtime().modify(function, true)?,
                    address,
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![extra_value_1, extra_value_2, extra_value_3, extra_value_4],
                );
            }
            Some(revive_common::ERAVM_ADDRESS_SET_CONTEXT_VALUE_CALL) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "set_context_value",
                )?;

                let value = value.expect("Always exists");

                return crate::eravm::extensions::general::set_context_value(context, value);
            }
            Some(revive_common::ERAVM_ADDRESS_SET_PUBDATA_PRICE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "set_pubdata_price",
                )?;

                let price = gas;

                return crate::eravm::extensions::general::set_pubdata_price(context, price);
            }
            Some(revive_common::ERAVM_ADDRESS_INCREMENT_TX_COUNTER) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "increment_tx_counter",
                )?;

                return crate::eravm::extensions::general::increment_tx_counter(context);
            }
            Some(revive_common::ERAVM_ADDRESS_GET_GLOBAL_PTR_CALLDATA) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "get_global_ptr_calldata",
                )?;

                let pointer = context.get_global_value(crate::eravm::GLOBAL_CALLDATA_POINTER)?;
                let value = context.builder().build_ptr_to_int(
                    pointer.into_pointer_value(),
                    context.field_type(),
                    "calldata_abi_integer",
                )?;
                return Ok(value.as_basic_value_enum());
            }
            Some(revive_common::ERAVM_ADDRESS_GET_GLOBAL_CALL_FLAGS) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "get_global_call_flags",
                )?;

                return context.get_global_value(crate::eravm::GLOBAL_CALL_FLAGS);
            }
            Some(revive_common::ERAVM_ADDRESS_GET_GLOBAL_PTR_RETURN_DATA) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "get_global_ptr_return_data",
                )?;

                let pointer = context.get_global_value(crate::eravm::GLOBAL_RETURN_DATA_POINTER)?;
                let value = context.builder().build_ptr_to_int(
                    pointer.into_pointer_value(),
                    context.field_type(),
                    "return_data_abi_integer",
                )?;
                return Ok(value.as_basic_value_enum());
            }
            Some(revive_common::ERAVM_ADDRESS_EVENT_INITIALIZE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "event_initialize",
                )?;

                let operand_1 = gas;
                let operand_2 = value.expect("Always exists");

                return crate::eravm::extensions::general::event(
                    context, operand_1, operand_2, true,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_EVENT_WRITE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().far_call,
                    function,
                    "event_initialize",
                )?;

                let operand_1 = gas;
                let operand_2 = value.expect("Always exists");

                return crate::eravm::extensions::general::event(
                    context, operand_1, operand_2, false,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_LOAD_CALLDATA) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_load_calldata",
                )?;

                return crate::eravm::extensions::abi::calldata_ptr_to_active(context);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_LOAD_RETURN_DATA) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_load_return_data",
                )?;

                return crate::eravm::extensions::abi::return_data_ptr_to_active(context);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_ADD) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_add",
                )?;

                let offset = gas;

                return crate::eravm::extensions::abi::active_ptr_add_assign(context, offset);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_SHRINK) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_shrink",
                )?;

                let offset = gas;

                return crate::eravm::extensions::abi::active_ptr_shrink_assign(context, offset);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_PACK) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_pack",
                )?;

                let data = gas;

                return crate::eravm::extensions::abi::active_ptr_pack_assign(context, data);
            }
            Some(revive_common::ERAVM_ADDRESS_MULTIPLICATION_HIGH_REGISTER) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "multiplication_high_register",
                )?;

                let operand_1 = gas;
                let operand_2 = input_offset;

                return crate::eravm::extensions::math::multiplication_512(
                    context, operand_1, operand_2,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_GET_GLOBAL_EXTRA_ABI_DATA) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "get_global_extra_abi_data",
                )?;

                let index = gas;

                return crate::eravm::extensions::abi::get_extra_abi_data(context, index);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_DATA_LOAD) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_data_load",
                )?;

                let offset = gas;

                return crate::eravm::extensions::abi::active_ptr_data_load(context, offset);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_DATA_SIZE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_data_size",
                )?;

                return crate::eravm::extensions::abi::active_ptr_data_size(context);
            }
            Some(revive_common::ERAVM_ADDRESS_ACTIVE_PTR_DATA_COPY) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "active_ptr_data_copy",
                )?;

                let destination_offset = gas;
                let source_offset = input_offset;
                let size = input_length;

                return crate::eravm::extensions::abi::active_ptr_data_copy(
                    context,
                    destination_offset,
                    source_offset,
                    size,
                );
            }
            Some(revive_common::ERAVM_ADDRESS_CONST_ARRAY_DECLARE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "const_array_declare",
                )?;

                let index = constants
                    .get_mut(0)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array index is missing"))?
                    .to_u8()
                    .ok_or_else(|| anyhow::anyhow!("Const array index must fit into 8 bits"))?;
                let size = constants
                    .get_mut(2)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array size is missing"))?
                    .to_u16()
                    .ok_or_else(|| anyhow::anyhow!("Const array size must fit into 16 bits"))?;

                return crate::eravm::extensions::const_array::declare(context, index, size);
            }
            Some(revive_common::ERAVM_ADDRESS_CONST_ARRAY_SET) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "const_array_set",
                )?;

                let index = constants
                    .get_mut(0)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array index is missing"))?
                    .to_u8()
                    .ok_or_else(|| anyhow::anyhow!("Const array index must fit into 8 bits"))?;
                let offset = constants
                    .get_mut(2)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array offset is missing"))?
                    .to_u16()
                    .ok_or_else(|| anyhow::anyhow!("Const array offset must fit into 16 bits"))?;
                let value = constants
                    .get_mut(4)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array assigned value is missing"))?;

                return crate::eravm::extensions::const_array::set(context, index, offset, value);
            }
            Some(revive_common::ERAVM_ADDRESS_CONST_ARRAY_FINALIZE) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "const_array_finalize",
                )?;

                let index = constants
                    .get_mut(0)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array index is missing"))?
                    .to_u8()
                    .ok_or_else(|| anyhow::anyhow!("Const array index must fit into 8 bits"))?;

                return crate::eravm::extensions::const_array::finalize(context, index);
            }
            Some(revive_common::ERAVM_ADDRESS_CONST_ARRAY_GET) => {
                crate::eravm::extensions::call::validate_call_type(
                    context.llvm_runtime().static_call,
                    function,
                    "const_array_get",
                )?;

                let index = constants
                    .get_mut(0)
                    .and_then(|option| option.take())
                    .ok_or_else(|| anyhow::anyhow!("Const array index is missing"))?
                    .to_u8()
                    .ok_or_else(|| anyhow::anyhow!("Const array index must fit into 8 bits"))?;
                let offset = input_offset;

                return crate::eravm::extensions::const_array::get(context, index, offset);
            }
            _ => {}
        }
    }

    let identity_block = context.append_basic_block("contract_call_identity_block");
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

///
/// Translates the Yul `linkersymbol` instruction.
///
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

///
/// Generates a custom request to a system contract.
///
pub fn request<'ctx, D>(
    context: &mut Context<'ctx, D>,
    address: inkwell::values::IntValue<'ctx>,
    signature: &'static str,
    arguments: Vec<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let signature_hash = crate::eravm::utils::keccak256(signature.as_bytes());
    let signature_value = context.field_const_str_hex(signature_hash.as_str());

    let calldata_size = context.field_const(
        (revive_common::BYTE_LENGTH_X32 + (revive_common::BYTE_LENGTH_FIELD * arguments.len()))
            as u64,
    );

    let calldata_array_pointer = context.build_alloca(
        context.array_type(context.field_type(), arguments.len()),
        "system_request_calldata_array_pointer",
    );
    for (index, argument) in arguments.into_iter().enumerate() {
        let argument_pointer = context.build_gep(
            calldata_array_pointer,
            &[context.field_const(0), context.field_const(index as u64)],
            context.field_type(),
            "system_request_calldata_array_pointer",
        );
        context.build_store(argument_pointer, argument)?;
    }
    Ok(context
        .build_invoke(
            context.llvm_runtime().system_request,
            &[
                address.as_basic_value_enum(),
                signature_value.as_basic_value_enum(),
                calldata_size.as_basic_value_enum(),
                calldata_array_pointer.value.as_basic_value_enum(),
            ],
            "system_request_call",
        )
        .expect("Always exists"))
}

///
/// The default call wrapper, which redirects the call to the `msg.value` simulator if `msg.value`
/// is not zero.
///
#[allow(clippy::too_many_arguments)]
fn _default_wrapped<'ctx, D>(
    context: &mut Context<'ctx, D>,
    function: FunctionDeclaration<'ctx>,
    gas: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let value_zero_block = context.append_basic_block("contract_call_value_zero_block");
    let value_non_zero_block = context.append_basic_block("contract_call_value_non_zero_block");
    let value_join_block = context.append_basic_block("contract_call_value_join_block");

    let result_pointer =
        context.build_alloca(context.field_type(), "contract_call_address_result_pointer");
    context.build_store(result_pointer, context.field_const(0))?;
    let is_value_zero = context.builder().build_int_compare(
        inkwell::IntPredicate::EQ,
        value,
        context.field_const(0),
        "contract_call_is_value_zero",
    )?;
    context.build_conditional_branch(is_value_zero, value_zero_block, value_non_zero_block)?;

    context.set_basic_block(value_non_zero_block);
    let abi_data = crate::eravm::utils::abi_data(
        context,
        input_offset,
        input_length,
        Some(gas),
        AddressSpace::Heap,
        true,
    )?;
    let result = crate::eravm::extensions::call::system(
        context,
        context.llvm_runtime().modify(function, false)?,
        context.field_const(zkevm_opcode_defs::ADDRESS_MSG_VALUE.into()),
        abi_data,
        output_offset,
        output_length,
        vec![
            value,
            address,
            context.field_const(u64::from(crate::eravm::r#const::NO_SYSTEM_CALL_BIT)),
        ],
    )?;
    context.build_store(result_pointer, result)?;
    context.build_unconditional_branch(value_join_block);

    context.set_basic_block(value_zero_block);
    let function = Runtime::default_call(context, function);
    let result = context
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
        .expect("Always exists");
    context.build_store(result_pointer, result)?;
    context.build_unconditional_branch(value_join_block);

    context.set_basic_block(value_join_block);
    context.build_load(result_pointer, "contract_call_address_result")
}

///
/// Generates a memory copy loop repeating the behavior of the EVM `Identity` precompile.
///
fn _identity<'ctx, D>(
    context: &mut Context<'ctx, D>,
    destination: inkwell::values::IntValue<'ctx>,
    source: inkwell::values::IntValue<'ctx>,
    size: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        destination,
        "contract_call_identity_destination",
    );
    let source = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        source,
        "contract_call_identity_source",
    );

    context.build_memcpy(
        context.intrinsics().memory_copy,
        destination,
        source,
        size,
        "contract_call_memcpy_to_child",
    )?;

    Ok(context.field_const(1).as_basic_value_enum())
}
