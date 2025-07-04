//! Translates the mathematical operations.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `addmod` instruction.
pub fn add_mod<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    modulo: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let result_pointer =
        context.build_alloca_at_entry(context.word_type(), "addmod_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "addmod_operand_1");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "addmod_operand_2");
    let modulo_pointer =
        context.build_alloca_at_entry(context.word_type(), "addmod_modulo_operand");

    context.build_store(operand_2_pointer, operand_2)?;
    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(modulo_pointer, modulo)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
        modulo_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::ADDMOD, arguments);
    context.build_load(result_pointer, "addmod_result")
}

/// Translates the `mulmod` instruction.
pub fn mul_mod<'ctx, D>(
    context: &mut Context<'ctx, D>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    modulo: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let result_pointer =
        context.build_alloca_at_entry(context.word_type(), "mulmod_result_pointer");
    let operand_1_pointer = context.build_alloca_at_entry(context.word_type(), "mulmod_operand_1");
    let operand_2_pointer = context.build_alloca_at_entry(context.word_type(), "mulmod_operand_2");
    let modulo_pointer =
        context.build_alloca_at_entry(context.word_type(), "mulmod_modulo_operand");

    context.build_store(operand_2_pointer, operand_2)?;
    context.build_store(operand_1_pointer, operand_1)?;
    context.build_store(modulo_pointer, modulo)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        operand_1_pointer.to_int(context).into(),
        operand_2_pointer.to_int(context).into(),
        modulo_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::MULMOD, arguments);
    context.build_load(result_pointer, "addmod_result")
}

/// Translates the `exp` instruction.
pub fn exponent<'ctx, D>(
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
    exponent: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let result_pointer = context.build_alloca_at_entry(context.word_type(), "exp_result_pointer");
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "exp_value_pointer");
    let exponent_pointer =
        context.build_alloca_at_entry(context.word_type(), "exp_exponent_pointer");

    context.build_store(value_pointer, value)?;
    context.build_store(exponent_pointer, exponent)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        value_pointer.to_int(context).into(),
        exponent_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::EXP, arguments);
    context.build_load(result_pointer, "exponent_result")
}

/// Translates the `signextend` instruction.
pub fn sign_extend<'ctx, D>(
    context: &mut Context<'ctx, D>,
    bytes: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let result_pointer =
        context.build_alloca_at_entry(context.word_type(), "signext_result_pointer");
    let bytes_pointer = context.build_alloca_at_entry(context.word_type(), "bytes_pointer");
    let value_pointer = context.build_alloca_at_entry(context.word_type(), "signext_value_pointer");

    context.build_store(bytes_pointer, bytes)?;
    context.build_store(value_pointer, value)?;

    let arguments = &[
        result_pointer.to_int(context).into(),
        bytes_pointer.to_int(context).into(),
        value_pointer.to_int(context).into(),
    ];

    context.build_runtime_call(revive_runtime_api::polkavm_imports::EXP, arguments);
    context.build_load(result_pointer, "signext_mod_result")
}
