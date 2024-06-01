//! Translates the contract creation instructions.

use inkwell::values::BasicValue;
use num::Zero;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm_const::runtime_api;

/// Translates the contract `create` instruction.
/// The instruction is simulated by a call to a system contract.
pub fn create<'ctx, D>(
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    self::create2(context, value, input_offset, input_length, None)
}

/// Translates the contract `create2` instruction.
pub fn create2<'ctx, D>(
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    salt: Option<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;

    let value_pointer = context.build_alloca(context.value_type(), "value");
    context.build_store(value_pointer, value)?;

    let code_hash_pointer = context.build_heap_gep(input_offset, input_length)?;

    let input_data_pointer = context.build_gep(
        code_hash_pointer,
        &[context
            .xlen_type()
            .const_int(revive_common::BYTE_LENGTH_WORD as u64, false)],
        context.byte_type(),
        "value_ptr_parameter_offset",
    );

    let salt_pointer = context.build_alloca(context.word_type(), "salt");
    context.build_store(salt_pointer, salt.unwrap_or_else(|| context.word_const(0)))?;

    let (address_pointer, address_length_pointer) =
        context.build_stack_parameter(revive_common::BIT_LENGTH_ETH_ADDRESS, "address_pointer");
    context.build_store(address_pointer, context.word_const(0))?;

    let argument_pointer = pallet_contracts_pvm_llapi::calling_convention::Spill::new(
        context.builder(),
        pallet_contracts_pvm_llapi::calling_convention::instantiate(context.llvm()),
        "create2_arguments",
    )?
    .next(code_hash_pointer.value)?
    .skip()
    .skip()
    .next(context.sentinel_pointer())?
    .next(value_pointer.value)?
    .next(input_data_pointer.value)?
    .next(input_length)?
    .next(address_pointer.value)?
    .next(address_length_pointer.value)?
    .next(context.sentinel_pointer())?
    .next(context.sentinel_pointer())?
    .next(salt_pointer.value)?
    .next(
        context
            .xlen_type()
            .const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
    )?
    .done();

    context.build_runtime_call(
        runtime_api::imports::INSTANTIATE,
        &[context
            .builder()
            .build_ptr_to_int(argument_pointer, context.xlen_type(), "argument_pointer")?
            .into()],
    );

    context.build_load_word(
        address_pointer,
        revive_common::BIT_LENGTH_ETH_ADDRESS,
        "address",
    )
}

/// Translates the contract hash instruction, which is actually used to set the hash of the contract
/// being created, or other related auxiliary data.
/// Represents `dataoffset` in Yul and `PUSH [$]` in the EVM legacy assembly.
pub fn contract_hash<'ctx, D>(
    context: &mut Context<'ctx, D>,
    identifier: String,
) -> anyhow::Result<Argument<'ctx>>
where
    D: Dependency + Clone,
{
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;

    let parent = context.module().get_name().to_str().expect("Always valid");

    let contract_path =
        context
            .resolve_path(identifier.as_str())
            .map_err(|error| match code_type {
                CodeType::Runtime if identifier.ends_with("_deployed") => {
                    anyhow::anyhow!("type({}).runtimeCode is not supported", identifier)
                }
                _ => error,
            })?;
    if contract_path.as_str() == parent {
        return Ok(Argument::new_with_constant(
            context.word_const(0).as_basic_value_enum(),
            num::BigUint::zero(),
        ));
    } else if identifier.ends_with("_deployed") && code_type == CodeType::Runtime {
        anyhow::bail!("type({}).runtimeCode is not supported", identifier);
    }

    let hash_string = context.compile_dependency(identifier.as_str())?;
    let hash_value = context
        .word_const_str_hex(hash_string.as_str())
        .as_basic_value_enum();
    Ok(Argument::new_with_original(hash_value, hash_string))
}

/// Translates the deploy call header size instruction. the header consists of
/// the hash of the bytecode of the contract whose instance is being created.
/// Represents `datasize` in Yul and `PUSH #[$]` in the EVM legacy assembly.
pub fn header_size<'ctx, D>(
    context: &mut Context<'ctx, D>,
    identifier: String,
) -> anyhow::Result<Argument<'ctx>>
where
    D: Dependency + Clone,
{
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;

    let parent = context.module().get_name().to_str().expect("Always valid");

    let contract_path =
        context
            .resolve_path(identifier.as_str())
            .map_err(|error| match code_type {
                CodeType::Runtime if identifier.ends_with("_deployed") => {
                    anyhow::anyhow!("type({}).runtimeCode is not supported", identifier)
                }
                _ => error,
            })?;
    if contract_path.as_str() == parent {
        return Ok(Argument::new_with_constant(
            context.word_const(0).as_basic_value_enum(),
            num::BigUint::zero(),
        ));
    } else if identifier.ends_with("_deployed") && code_type == CodeType::Runtime {
        anyhow::bail!("type({}).runtimeCode is not supported", identifier);
    }

    let size_bigint = num::BigUint::from(crate::polkavm::DEPLOYER_CALL_HEADER_SIZE);
    let size_value = context
        .word_const(crate::polkavm::DEPLOYER_CALL_HEADER_SIZE as u64)
        .as_basic_value_enum();
    Ok(Argument::new_with_constant(size_value, size_bigint))
}
