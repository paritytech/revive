//! Translates the mathematical operations.
//!
//! Each of these four EVM operations has an instruction of its own under the wide integer
//! extension, reached through an intrinsic. A build linked against an LLVM without the
//! extension falls back to the `stdlib.ll` routines they replace, which together are over
//! ten kilobytes of code.

use inkwell::values::BasicValue;

use crate::polkavm::context::Context;

/// Translates the `addmod` instruction.
pub fn add_mod<'ctx>(
    context: &mut Context<'ctx>,
    operand_1: inkwell::values::IntValue<'ctx>,
    operand_2: inkwell::values::IntValue<'ctx>,
    modulo: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let function = context
        .intrinsics()
        .wide_add_mod
        .unwrap_or(context.llvm_runtime().add_mod);

    Ok(context
        .build_call(
            function,
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
    let function = context
        .intrinsics()
        .wide_mul_mod
        .unwrap_or(context.llvm_runtime().mul_mod);

    Ok(context
        .build_call(
            function,
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
    let function = context
        .intrinsics()
        .wide_exponent
        .unwrap_or(context.llvm_runtime().exp);

    Ok(context
        .build_call(
            function,
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
    // The instruction takes the value it operates on first, as the shifts do, which is the
    // opposite of the order the EVM opcode and the `stdlib.ll` routine use.
    if let Some(function) = context.intrinsics().wide_sign_extend {
        return Ok(context
            .build_call(
                function,
                &[value.as_basic_value_enum(), bytes.as_basic_value_enum()],
                "sign_extend_call",
            )
            .expect("Always exists"));
    }

    Ok(context
        .build_call(
            context.llvm_runtime().sign_extend,
            &[bytes.as_basic_value_enum(), value.as_basic_value_enum()],
            "sign_extend_call",
        )
        .expect("Always exists"))
}
