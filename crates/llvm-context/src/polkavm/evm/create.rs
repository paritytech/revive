//! Translates the contract creation instructions.

use inkwell::values::BasicValue;
use num::Zero;

use revive_common::BIT_LENGTH_ETH_ADDRESS;

use crate::polkavm::context::argument::Argument;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::Context;

/// Translates the contract `create` and `create2` instruction.
///
/// A `salt` value of `None` is equivalent to `create1`.
pub fn create<'ctx>(
    context: &mut Context<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    salt: Option<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let input_offset = context.safe_truncate_int_to_xlen(input_offset)?;
    let input_length = context.safe_truncate_int_to_xlen(input_length)?;

    let code_hash_pointer = context.build_heap_gep(input_offset, input_length)?;

    let value_pointer = context.build_alloca_at_entry(context.value_type(), "transferred_value");
    context.build_store(value_pointer, value)?;

    let salt_pointer = match salt {
        Some(salt) => {
            let salt_pointer = context.build_alloca_at_entry(context.word_type(), "salt_pointer");
            let salt = context.build_byte_swap(salt.into())?;
            context.build_store(salt_pointer, salt)?;
            salt_pointer
        }
        None => context.sentinel_pointer(),
    };

    let address_pointer = context.build_alloca_at_entry(
        context.integer_type(BIT_LENGTH_ETH_ADDRESS),
        "address_pointer",
    );
    context.build_store(address_pointer, context.word_const(0))?;

    let deposit_pointer = context.build_alloca_at_entry(context.word_type(), "deposit_pointer");
    context.build_store(deposit_pointer, context.word_type().const_all_ones())?;

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
        code_hash_pointer.to_int(context),
        "input_data",
    )?;
    let address_and_salt = revive_runtime_api::calling_convention::pack_hi_lo_reg(
        context.builder(),
        context.llvm(),
        address_pointer.to_int(context),
        salt_pointer.to_int(context),
        "output_data",
    )?;

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::INSTANTIATE,
        &[
            context.register_type().const_all_ones().into(),
            context.register_type().const_all_ones().into(),
            deposit_and_value.into(),
            input_data.into(),
            context.register_type().const_all_ones().into(),
            address_and_salt.into(),
        ],
    );

    let address = context.build_byte_swap(context.build_load(address_pointer, "address")?)?;
    Ok(context
        .builder()
        .build_int_z_extend(
            address.into_int_value(),
            context.word_type(),
            "address_zext",
        )?
        .into())
}

/// Translates the contract hash instruction, which is actually used to set the hash of the contract
/// being created, or other related auxiliary data.
/// Represents `dataoffset` in Yul and `PUSH [$]` in the EVM legacy assembly.
pub fn contract_hash<'ctx>(
    context: &mut Context<'ctx>,
    identifier: String,
) -> anyhow::Result<Argument<'ctx>> {
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;

    let parent = context.module().get_name().to_str().expect("Always valid");

    let full_path = match context.yul() {
        Some(yul_data) => yul_data
            .resolve_path(
                identifier
                    .strip_suffix("_deployed")
                    .unwrap_or(identifier.as_str()),
            )
            .expect("Always exists")
            .to_owned(),
        None => identifier.clone(),
    };

    match code_type {
        CodeType::Deploy if full_path == parent => {
            return Ok(Argument::value(context.word_const(0).as_basic_value_enum())
                .with_constant(num::BigUint::zero()));
        }
        CodeType::Runtime if context.yul().is_some() && identifier.ends_with("_deployed") => {
            anyhow::bail!("type({identifier}).runtimeCode is not supported");
        }
        _ => {}
    }

    context.declare_global(&full_path, context.word_type(), Default::default());
    context
        .build_load(context.get_global(&full_path)?.into(), &full_path)
        .map(Argument::value)
}

/// Translates the deploy call header size instruction. the header consists of
/// the hash of the bytecode of the contract whose instance is being created.
/// Represents `datasize` in Yul and `PUSH #[$]` in the EVM legacy assembly.
pub fn header_size<'ctx>(
    context: &mut Context<'ctx>,
    identifier: String,
) -> anyhow::Result<Argument<'ctx>> {
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;

    let parent = context.module().get_name().to_str().expect("Always valid");

    let full_path = match context.yul() {
        Some(yul_data) => yul_data
            .resolve_path(
                identifier
                    .strip_suffix("_deployed")
                    .unwrap_or(identifier.as_str()),
            )
            .unwrap_or_else(|| panic!("ICE: {identifier} not found {yul_data:?}")),
        None => identifier.as_str(),
    };

    match code_type {
        CodeType::Deploy if full_path == parent => {
            return Ok(Argument::value(context.word_const(0).as_basic_value_enum())
                .with_constant(num::BigUint::zero()));
        }
        CodeType::Runtime if context.yul().is_some() && identifier.ends_with("_deployed") => {
            anyhow::bail!("type({identifier}).runtimeCode is not supported");
        }
        _ => {}
    }

    let size_value = context
        .word_const(crate::polkavm::DEPLOYER_CALL_HEADER_SIZE as u64)
        .as_basic_value_enum();
    let size_bigint = num::BigUint::from(crate::polkavm::DEPLOYER_CALL_HEADER_SIZE);
    Ok(Argument::value(size_value).with_constant(size_bigint))
}
