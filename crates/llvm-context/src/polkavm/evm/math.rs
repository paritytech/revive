//! Translates the mathematical operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;

/// Translates the `addmod` instruction.
pub fn add_mod<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    modulo: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .build_call(
            context.llvm_runtime().add_mod,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
                modulo.as_basic_value_enum(),
            ],
            "add_mod_call",
        )
        .expect("Always exists"))
}

/// Translates the `mulmod` instruction.
pub fn mul_mod<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    modulo: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .build_call(
            context.llvm_runtime().mul_mod,
            &[
                operand_1.as_basic_value_enum(),
                operand_2.as_basic_value_enum(),
                modulo.as_basic_value_enum(),
            ],
            "mul_mod_call",
        )
        .expect("Always exists"))
}

/// Translates the `exp` instruction.
pub fn exponent<'ctx>(
    context: &mut Context<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
    exponent: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .build_call(
            context.llvm_runtime().exp,
            &[value.as_basic_value_enum(), exponent.as_basic_value_enum()],
            "exp_call",
        )
        .expect("Always exists"))
}

/// Translates the `signextend` instruction.
pub fn sign_extend<'ctx>(
    context: &mut Context<'ctx>,
    bytes: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    Ok(context
        .build_call(
            context.llvm_runtime().sign_extend,
            &[bytes.as_basic_value_enum(), value.as_basic_value_enum()],
            "sign_extend_call",
        )
        .expect("Always exists"))
}
