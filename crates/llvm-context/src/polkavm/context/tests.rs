//! The LLVM IR generator context tests.

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::polkavm::context::{attribute::Attribute, function::llvm_runtime::LLVMRuntime, Context};
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

#[test]
pub fn custom_wide_instruction_intrinsics_exist() {
    assert!(
        inkwell::intrinsics::Intrinsic::find("llvm.riscv.revive.mulmod").is_some(),
        "the linked LLVM does not carry the custom wide instruction intrinsics"
    );
}

/// The wide setting swaps the modular arithmetic helpers for intrinsics and makes the routines
/// they replace reclaimable. `stdlib.ll` gives these external linkage, and global DCE cannot
/// drop an externally visible symbol, so without the demotion they would survive in every emitted object.
#[test]
pub fn custom_wide_instructions_replace_the_stdlib_modular_arithmetic() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let mut settings = OptimizerSettings::size();
    settings.wide_instructions = true;
    let context = Context::new_dummy(&llvm, settings);

    assert_eq!(
        context
            .llvm_runtime()
            .mul_mod
            .value
            .get_name()
            .to_str()
            .expect("should get the intrinsic name"),
        LLVMRuntime::INTRINSIC_MULMOD,
    );
    assert_eq!(
        context
            .module()
            .get_function(LLVMRuntime::FUNCTION_MULMOD)
            .expect("stdlib is linked into every module")
            .get_linkage(),
        inkwell::module::Linkage::Private,
    );
}

/// The default path still binds the stdlib routines, and still privatizes them.
#[test]
pub fn default_settings_keep_the_stdlib_modular_arithmetic() {
    initialize_llvm();

    let llvm = inkwell::context::Context::create();
    let context = Context::new_dummy(&llvm, OptimizerSettings::size());

    assert_eq!(
        context
            .llvm_runtime()
            .mul_mod
            .value
            .get_name()
            .to_str()
            .expect("should get the function name"),
        LLVMRuntime::FUNCTION_MULMOD,
    );
    assert_eq!(
        context
            .module()
            .get_function(LLVMRuntime::FUNCTION_MULMOD)
            .expect("stdlib is linked into every module")
            .get_linkage(),
        inkwell::module::Linkage::Private,
    );
}
