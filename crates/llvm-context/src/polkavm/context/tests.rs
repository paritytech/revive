//! The LLVM IR generator context tests.

use inkwell::values::InstructionOpcode;
use num::BigUint;

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::constant_division;
use crate::polkavm::context::Context;
use crate::PolkaVMTarget;

/// Initializes the LLVM compiler backend.
fn initialize_llvm() {
    crate::initialize_llvm(
        PolkaVMTarget::PVM,
        "resolc",
        crate::OptimizerSettingsSizeLevel::Zero,
        false,
        Default::default(),
    );
}

#[test]
pub fn check_attribute_null_pointer_is_invalid() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut context = Context::new_dummy(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
            None,
            false,
        )
        .expect("Failed to add function");
    assert!(!function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::NullPointerIsValid as u32, 0)));
}

#[test]
pub fn check_attribute_optimize_for_size_mode_3() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut context = Context::new_dummy(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
            None,
            false,
        )
        .expect("Failed to add function");
    assert!(!function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::OptimizeForSize as u32, 0)));
}

#[test]
pub fn check_attribute_optimize_for_size_mode_z() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut context = Context::new_dummy(&llvm, OptimizerSettings::size());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
            None,
            false,
        )
        .expect("Failed to add function");
    assert!(function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::OptimizeForSize as u32, 0)));
}

#[test]
pub fn check_attribute_min_size_mode_3() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut context = Context::new_dummy(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
            None,
            false,
        )
        .expect("Failed to add function");
    assert!(!function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::MinSize as u32, 0)));
}

#[test]
pub fn check_attribute_min_size_mode_z() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut context = Context::new_dummy(&llvm, OptimizerSettings::size());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
            None,
            false,
        )
        .expect("Failed to add function");
    assert!(function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::MinSize as u32, 0)));
}

/// Adds an `i256 name(i256 x parameter_count)` function with an empty entry
/// block to the dummy context's module.
fn add_word_function<'ctx>(
    llvm: &'ctx inkwell::context::Context,
    context: &Context<'ctx>,
    name: &str,
    parameter_count: usize,
) -> inkwell::values::FunctionValue<'ctx> {
    let word_type = context.word_type();
    let parameter_types = vec![word_type.into(); parameter_count];
    let function =
        context
            .module()
            .add_function(name, word_type.fn_type(&parameter_types, false), None);
    llvm.append_basic_block(function, "entry");
    function
}

/// Counts the call instructions in `function` whose callee is `callee`,
/// compared by pointer identity.
fn count_calls_to<'ctx>(
    function: inkwell::values::FunctionValue<'ctx>,
    callee: inkwell::values::FunctionValue<'ctx>,
) -> usize {
    let callee_pointer = callee.as_global_value().as_pointer_value();
    let mut count = 0;
    for basic_block in function.get_basic_blocks() {
        for instruction in basic_block.get_instructions() {
            if instruction.get_opcode() != InstructionOpcode::Call {
                continue;
            }
            let last_operand = instruction
                .get_operand(instruction.get_num_operands() - 1)
                .and_then(|operand| operand.value());
            if last_operand.is_some_and(|operand| {
                operand.is_pointer_value() && operand.into_pointer_value() == callee_pointer
            }) {
                count += 1;
            }
        }
    }
    count
}

/// Counts the instructions in `function` with the given opcode; when
/// `bit_width` is given, only instructions of that integer width count.
fn count_opcodes(
    function: inkwell::values::FunctionValue,
    opcode: InstructionOpcode,
    bit_width: Option<u32>,
) -> usize {
    let mut count = 0;
    for basic_block in function.get_basic_blocks() {
        for instruction in basic_block.get_instructions() {
            if instruction.get_opcode() != opcode {
                continue;
            }
            if let Some(bit_width) = bit_width {
                if !instruction.get_type().is_int_type()
                    || instruction.get_type().into_int_type().get_bit_width() != bit_width
                {
                    continue;
                }
            }
            count += 1;
        }
    }
    count
}

/// Counts the module functions whose name starts with the reserved Barrett
/// helper prefix.
fn count_barrett_helpers(context: &Context) -> usize {
    context
        .module()
        .get_functions()
        .filter(|function| {
            function
                .get_name()
                .to_string_lossy()
                .starts_with(constant_division::HELPER_PREFIX)
        })
        .count()
}

/// The secp256k1 field prime: a 256-bit constant with all limbs populated.
fn secp256k1_prime() -> BigUint {
    BigUint::parse_bytes(
        b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f",
        16,
    )
    .expect("the secp256k1 prime is valid hexadecimal")
}

#[test]
fn barrett_rewrites_constant_unsigned_division() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    // 2^128 + 1: not narrowable, not a power of two, takes the multiply path.
    let divisor = (BigUint::from(1u32) << 128u32) + 1u32;
    let function = add_word_function(&llvm, &context, "test", 1);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let dividend = function.get_first_param().unwrap().into_int_value();
    let quotient = builder
        .build_int_unsigned_div(dividend, Context::biguint_constant(word_type, &divisor), "")
        .unwrap();
    builder.build_return(Some(&quotient)).unwrap();

    context.lower_wide_division();

    let helper_name = format!(
        "{}{}{}",
        constant_division::HELPER_PREFIX,
        constant_division::UDIV_HELPER_INFIX,
        divisor.to_str_radix(16)
    );
    let helper = context
        .module()
        .get_function(&helper_name)
        .expect("the Barrett division helper must have been generated");
    assert_eq!(helper.get_linkage(), inkwell::module::Linkage::Internal);
    assert!(helper
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::NoInline as u32, 0)));
    assert_eq!(count_calls_to(function, helper), 1);
    assert_eq!(count_opcodes(function, InstructionOpcode::UDiv, None), 0);
    context.verify().unwrap();
}

#[test]
fn barrett_rewrites_constant_unsigned_remainder() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    let divisor = secp256k1_prime();
    let function = add_word_function(&llvm, &context, "test", 1);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let dividend = function.get_first_param().unwrap().into_int_value();
    let remainder = builder
        .build_int_unsigned_rem(dividend, Context::biguint_constant(word_type, &divisor), "")
        .unwrap();
    builder.build_return(Some(&remainder)).unwrap();

    context.lower_wide_division();

    let helper_name = format!(
        "{}{}{}",
        constant_division::HELPER_PREFIX,
        constant_division::UREM_HELPER_INFIX,
        divisor.to_str_radix(16)
    );
    let helper = context
        .module()
        .get_function(&helper_name)
        .expect("the Barrett remainder helper must have been generated");
    assert_eq!(helper.get_linkage(), inkwell::module::Linkage::Internal);
    assert!(helper
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::NoInline as u32, 0)));
    assert_eq!(count_calls_to(function, helper), 1);
    assert_eq!(count_opcodes(function, InstructionOpcode::URem, None), 0);
    context.verify().unwrap();
}

#[test]
fn barrett_helper_deduplicated_across_functions() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();
    let builder = llvm.create_builder();

    let divisor = (BigUint::from(1u32) << 192u32) + 9u32;
    let mut functions = Vec::new();
    for name in ["test_one", "test_two"] {
        let function = add_word_function(&llvm, &context, name, 1);
        builder.position_at_end(function.get_first_basic_block().unwrap());
        let dividend = function.get_first_param().unwrap().into_int_value();
        let quotient = builder
            .build_int_unsigned_div(dividend, Context::biguint_constant(word_type, &divisor), "")
            .unwrap();
        builder.build_return(Some(&quotient)).unwrap();
        functions.push(function);
    }

    context.lower_wide_division();

    assert_eq!(count_barrett_helpers(&context), 1);
    let helper_name = format!(
        "{}{}{}",
        constant_division::HELPER_PREFIX,
        constant_division::UDIV_HELPER_INFIX,
        divisor.to_str_radix(16)
    );
    let helper = context.module().get_function(&helper_name).unwrap();
    for function in functions {
        assert_eq!(count_calls_to(function, helper), 1);
    }
    context.verify().unwrap();
}

#[test]
fn barrett_skips_powers_of_two_and_trivial_divisors() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();
    let builder = llvm.create_builder();

    let divisors = [
        BigUint::from(0u32),
        BigUint::from(1u32),
        BigUint::from(1u32) << 200u32,
    ];
    let mut functions = Vec::new();
    for (index, divisor) in divisors.iter().enumerate() {
        let function = add_word_function(&llvm, &context, &format!("test_{index}"), 1);
        builder.position_at_end(function.get_first_basic_block().unwrap());
        let dividend = function.get_first_param().unwrap().into_int_value();
        let quotient = builder
            .build_int_unsigned_div(dividend, Context::biguint_constant(word_type, divisor), "")
            .unwrap();
        builder.build_return(Some(&quotient)).unwrap();
        functions.push(function);
    }

    context.lower_wide_division();

    assert_eq!(count_barrett_helpers(&context), 0);
    let udiv256 = context.module().get_function("__udiv256").unwrap();
    for function in functions {
        assert_eq!(count_calls_to(function, udiv256), 1);
    }
    context.verify().unwrap();
}

#[test]
fn barrett_respects_narrowing_precedence() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    let function = add_word_function(&llvm, &context, "test", 1);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let parameter = function.get_first_param().unwrap().into_int_value();
    let masked = builder
        .build_and(parameter, word_type.const_int(u16::MAX as u64, false), "")
        .unwrap();
    let quotient = builder
        .build_int_unsigned_div(masked, word_type.const_int(5, false), "")
        .unwrap();
    builder.build_return(Some(&quotient)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_barrett_helpers(&context), 0);
    assert_eq!(
        count_opcodes(function, InstructionOpcode::UDiv, Some(256)),
        0
    );
    assert_eq!(
        count_opcodes(function, InstructionOpcode::UDiv, Some(16)),
        1
    );
    context.verify().unwrap();
}

#[test]
fn barrett_leaves_signed_constant_division_routed() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();
    let builder = llvm.create_builder();

    // A 256-bit magnitude defeats narrowing, keeping the sites on the ladder's
    // routing rung.
    let divisor = Context::biguint_constant(word_type, &secp256k1_prime());

    let division = add_word_function(&llvm, &context, "test_sdiv", 1);
    builder.position_at_end(division.get_first_basic_block().unwrap());
    let dividend = division.get_first_param().unwrap().into_int_value();
    let quotient = builder.build_int_signed_div(dividend, divisor, "").unwrap();
    builder.build_return(Some(&quotient)).unwrap();

    let remainder_function = add_word_function(&llvm, &context, "test_srem", 1);
    builder.position_at_end(remainder_function.get_first_basic_block().unwrap());
    let dividend = remainder_function
        .get_first_param()
        .unwrap()
        .into_int_value();
    let remainder = builder.build_int_signed_rem(dividend, divisor, "").unwrap();
    builder.build_return(Some(&remainder)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_barrett_helpers(&context), 0);
    let sdiv256 = context.module().get_function("__sdiv256").unwrap();
    let srem256 = context.module().get_function("__srem256").unwrap();
    assert_eq!(count_calls_to(division, sdiv256), 1);
    assert_eq!(count_calls_to(remainder_function, srem256), 1);
    context.verify().unwrap();
}

#[test]
fn barrett_handles_degenerate_full_width_divisor() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();
    let builder = llvm.create_builder();

    // 2^256 - 1: bits(C) == 256, the compare-and-subtract helper body.
    let divisor = (BigUint::from(1u32) << 256u32) - 1u32;

    let division = add_word_function(&llvm, &context, "test_udiv", 1);
    builder.position_at_end(division.get_first_basic_block().unwrap());
    let dividend = division.get_first_param().unwrap().into_int_value();
    let quotient = builder
        .build_int_unsigned_div(dividend, Context::biguint_constant(word_type, &divisor), "")
        .unwrap();
    builder.build_return(Some(&quotient)).unwrap();

    let remainder_function = add_word_function(&llvm, &context, "test_urem", 1);
    builder.position_at_end(remainder_function.get_first_basic_block().unwrap());
    let dividend = remainder_function
        .get_first_param()
        .unwrap()
        .into_int_value();
    let remainder = builder
        .build_int_unsigned_rem(dividend, Context::biguint_constant(word_type, &divisor), "")
        .unwrap();
    builder.build_return(Some(&remainder)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_barrett_helpers(&context), 2);
    assert_eq!(count_opcodes(division, InstructionOpcode::UDiv, None), 0);
    assert_eq!(
        count_opcodes(remainder_function, InstructionOpcode::URem, None),
        0
    );
    context.verify().unwrap();
}

#[test]
fn wide_unsigned_constant_round_trips_without_instructions() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    let function = add_word_function(&llvm, &context, "test", 1);
    let entry = function.get_first_basic_block().unwrap();
    let builder = llvm.create_builder();
    builder.position_at_end(entry);

    let expected = secp256k1_prime();
    let constant = Context::biguint_constant(word_type, &expected);
    let extracted = Context::wide_unsigned_constant(&builder, constant)
        .expect("the constant must be extractable");
    assert_eq!(extracted, expected);
    // Pins the fold-only contract: extraction must not insert instructions.
    assert_eq!(entry.get_instructions().count(), 0);
}

#[test]
fn mulmod_constant_modulus_rewrites_to_barrett_call() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    let modulus = secp256k1_prime();
    let function = add_word_function(&llvm, &context, "test", 2);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let mulmod = context.module().get_function("__mulmod").unwrap();
    let result = builder
        .build_call(
            mulmod,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                Context::biguint_constant(word_type, &modulus).into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();

    context.lower_wide_division();

    let mulmod_barrett = context.module().get_function("__mulmod_barrett").unwrap();
    assert_eq!(count_calls_to(function, mulmod), 0);
    assert_eq!(count_calls_to(function, mulmod_barrett), 1);

    // The rewritten call must carry floor(2^512 / m) - 2^256 as its fourth
    // argument (LLVM interns constants, so pointer equality is complete).
    let expected_reciprocal_low =
        (BigUint::from(1u32) << 512u32) / &modulus - (BigUint::from(1u32) << 256u32);
    let expected_constant = Context::biguint_constant(word_type, &expected_reciprocal_low);
    let call = function
        .get_basic_blocks()
        .into_iter()
        .flat_map(|basic_block| basic_block.get_instructions())
        .find(|instruction| instruction.get_opcode() == InstructionOpcode::Call)
        .expect("the rewritten call must exist");
    assert_eq!(call.get_num_operands(), 5);
    let reciprocal_argument = call
        .get_operand(3)
        .unwrap()
        .value()
        .unwrap()
        .into_int_value();
    assert_eq!(reciprocal_argument, expected_constant);
    context.verify().unwrap();
}

#[test]
fn mulmod_small_constant_modulus_stays_routed() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    // 2^128 + 1: bits == 129 < 256, below the sharp eligibility boundary.
    let modulus = (BigUint::from(1u32) << 128u32) + 1u32;
    let function = add_word_function(&llvm, &context, "test", 2);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let mulmod = context.module().get_function("__mulmod").unwrap();
    let result = builder
        .build_call(
            mulmod,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                Context::biguint_constant(word_type, &modulus).into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_calls_to(function, mulmod), 1);
    let mulmod_barrett = context.module().get_function("__mulmod_barrett").unwrap();
    assert_eq!(count_calls_to(function, mulmod_barrett), 0);
    context.verify().unwrap();
}

#[test]
fn mulmod_power_of_two_modulus_becomes_masked_multiply() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    // 2^255: the mu_lo-overflow case, must take the inline mask rewrite.
    let modulus = BigUint::from(1u32) << 255u32;
    let function = add_word_function(&llvm, &context, "test", 2);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let mulmod = context.module().get_function("__mulmod").unwrap();
    let result = builder
        .build_call(
            mulmod,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                Context::biguint_constant(word_type, &modulus).into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_opcodes(function, InstructionOpcode::Call, None), 0);
    assert_eq!(
        count_opcodes(function, InstructionOpcode::Mul, Some(256)),
        1
    );
    assert_eq!(
        count_opcodes(function, InstructionOpcode::And, Some(256)),
        1
    );
    let expected_mask = Context::biguint_constant(word_type, &(modulus - 1u32));
    let and_instruction = function
        .get_basic_blocks()
        .into_iter()
        .flat_map(|basic_block| basic_block.get_instructions())
        .find(|instruction| instruction.get_opcode() == InstructionOpcode::And)
        .unwrap();
    let mask_operand = and_instruction
        .get_operand(1)
        .unwrap()
        .value()
        .unwrap()
        .into_int_value();
    assert_eq!(mask_operand, expected_mask);
    context.verify().unwrap();
}

#[test]
fn mulmod_runtime_modulus_stays_untouched() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());

    let function = add_word_function(&llvm, &context, "test", 3);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let mulmod = context.module().get_function("__mulmod").unwrap();
    let result = builder
        .build_call(
            mulmod,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                function.get_nth_param(2).unwrap().into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_calls_to(function, mulmod), 1);
    assert_eq!(count_barrett_helpers(&context), 0);
    context.verify().unwrap();
}

#[test]
fn mulmod_unrelated_calls_stay_untouched() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::cycles());
    let word_type = context.word_type();

    // Same shape as an eligible __mulmod call, different callee.
    let unrelated = context.module().add_function(
        "unrelated_three_argument_function",
        word_type.fn_type(
            &[word_type.into(), word_type.into(), word_type.into()],
            false,
        ),
        None,
    );
    let function = add_word_function(&llvm, &context, "test", 2);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let result = builder
        .build_call(
            unrelated,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                Context::biguint_constant(word_type, &secp256k1_prime()).into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();

    context.lower_wide_division();

    assert_eq!(count_calls_to(function, unrelated), 1);
    let mulmod_barrett = context.module().get_function("__mulmod_barrett").unwrap();
    assert_eq!(count_calls_to(function, mulmod_barrett), 0);
    context.verify().unwrap();
}

#[test]
fn reciprocal_self_checks_hold_on_fuzz_matrix_constants() {
    let one = BigUint::from(1u32);

    // The adversarial mulmod modulus grid from the verification protocol.
    for modulus in [
        (&one << 255u32) + 1u32,
        (&one << 255u32) + 3u32,
        (&one << 256u32) - 1u32,
        (&one << 256u32) - 2u32,
        (&one << 256u32) - 3u32,
        secp256k1_prime(),
        (&one << 255u32) | &one,
        (&one << 255u32) | ((&one << 128u32) - 1u32),
        (&one << 255u32) + (&one << 254u32) + 1u32,
    ] {
        let reciprocal_low = Context::mulmod_reciprocal_low(&modulus);
        assert!(reciprocal_low >= one && reciprocal_low.bits() <= 256);
    }

    // The per-constant division/remainder fuzz list.
    for divisor in [
        BigUint::from(3u32),
        BigUint::from(5u32),
        BigUint::from(10u32),
        (&one << 64u32) + 1u32,
        (&one << 128u32) - 1u32,
        (&one << 128u32) + 1u32,
        (&one << 192u32) + 9u32,
        (&one << 255u32) + 3u32,
        (&one << 256u32) - 1u32,
    ] {
        let reciprocal = Context::division_reciprocal(&divisor);
        assert!(reciprocal >= one && reciprocal.bits() <= 256);
    }
}

/// Builds `__mulmod(x, y, xor(load(slot), -1))` in a fresh function, with
/// `not(modulus)` stored to a stack slot: the modulus operand is a real
/// instruction chain rather than a `ConstantInt`, modeling how solc emits
/// large constants in NOT-form.
fn add_composed_modulus_mulmod_function<'ctx>(
    llvm: &'ctx inkwell::context::Context,
    context: &Context<'ctx>,
    modulus: &BigUint,
) -> inkwell::values::FunctionValue<'ctx> {
    let word_type = context.word_type();
    let function = add_word_function(llvm, context, "test", 2);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());

    let all_ones = (BigUint::from(1u32) << 256u32) - 1u32;
    let negated_modulus = Context::biguint_constant(word_type, &(&all_ones ^ modulus));
    let slot = builder.build_alloca(word_type, "slot").unwrap();
    builder.build_store(slot, negated_modulus).unwrap();
    let loaded = builder
        .build_load(word_type, slot, "")
        .unwrap()
        .into_int_value();
    let composed_modulus = builder
        .build_xor(loaded, word_type.const_all_ones(), "")
        .unwrap();

    let mulmod = context.module().get_function("__mulmod").unwrap();
    let result = builder
        .build_call(
            mulmod,
            &[
                function.get_nth_param(0).unwrap().into(),
                function.get_nth_param(1).unwrap().into(),
                composed_modulus.into(),
            ],
            "",
        )
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    builder.build_return(Some(&result)).unwrap();
    function
}

#[test]
fn mulmod_composed_constant_modulus_specializes_before_pipeline() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let settings = OptimizerSettings::cycles();
    let context = Context::new_dummy(&llvm, settings.clone());
    let function = add_composed_modulus_mulmod_function(&llvm, &context, &secp256k1_prime());
    let mulmod = context.module().get_function("__mulmod").unwrap();
    assert_eq!(count_calls_to(function, mulmod), 1);

    let target_machine = crate::PolkaVMTargetMachine::new(PolkaVMTarget::PVM, &settings, false)
        .expect("the test target machine must be creatable");
    context
        .specialize_constant_modulus_mulmod(&target_machine)
        .expect("the pre-pipeline specialization must succeed");

    // The constant-fold pre-pass exposes the NOT-form modulus and the sweep
    // retargets the call, even though nothing was a `ConstantInt` when the
    // module was built.
    let mulmod_barrett = context.module().get_function("__mulmod_barrett").unwrap();
    assert_eq!(count_calls_to(function, mulmod), 0);
    assert_eq!(count_calls_to(function, mulmod_barrett), 1);
}

#[test]
fn mulmod_specialization_is_skipped_at_optimization_level_zero() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let settings = OptimizerSettings::none();
    let context = Context::new_dummy(&llvm, settings.clone());
    let function = add_composed_modulus_mulmod_function(&llvm, &context, &secp256k1_prime());

    let target_machine = crate::PolkaVMTargetMachine::new(PolkaVMTarget::PVM, &settings, false)
        .expect("the test target machine must be creatable");
    context
        .specialize_constant_modulus_mulmod(&target_machine)
        .expect("the pre-pipeline specialization must succeed");

    // At -O0 the hook returns before doing anything: the call still targets
    // `__mulmod` and the constant-fold pre-pass never ran (the stack slot
    // `mem2reg` would have promoted is still there).
    let mulmod = context.module().get_function("__mulmod").unwrap();
    let mulmod_barrett = context.module().get_function("__mulmod_barrett").unwrap();
    assert_eq!(count_calls_to(function, mulmod), 1);
    assert_eq!(count_calls_to(function, mulmod_barrett), 0);
    assert_eq!(count_opcodes(function, InstructionOpcode::Alloca, None), 1);
}

#[test]
fn mulmod_specialization_is_skipped_without_mulmod_calls() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let settings = OptimizerSettings::cycles();
    let context = Context::new_dummy(&llvm, settings.clone());

    // A function with foldable memory traffic but no `__mulmod` call.
    let word_type = context.word_type();
    let function = add_word_function(&llvm, &context, "test", 1);
    let builder = llvm.create_builder();
    builder.position_at_end(function.get_first_basic_block().unwrap());
    let slot = builder.build_alloca(word_type, "slot").unwrap();
    builder
        .build_store(slot, function.get_first_param().unwrap().into_int_value())
        .unwrap();
    let loaded = builder
        .build_load(word_type, slot, "")
        .unwrap()
        .into_int_value();
    builder.build_return(Some(&loaded)).unwrap();

    let target_machine = crate::PolkaVMTargetMachine::new(PolkaVMTarget::PVM, &settings, false)
        .expect("the test target machine must be creatable");
    context
        .specialize_constant_modulus_mulmod(&target_machine)
        .expect("the pre-pipeline specialization must succeed");

    // `__mulmod` exists in the module (the stdlib is always linked) but has
    // no uses, so the hook must skip the pre-pass entirely: the promotable
    // stack slot proves no optimization ran.
    assert_eq!(count_opcodes(function, InstructionOpcode::Alloca, None), 1);
}
