//! The LLVM IR generator context tests.

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::optimizer::Optimizer;
use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::Context;
use crate::polkavm::DummyDependency;

pub fn create_context(
    llvm: &inkwell::context::Context,
    optimizer_settings: OptimizerSettings,
) -> Context<DummyDependency> {
    crate::polkavm::initialize_target();

    let module = llvm.create_module("test");
    let optimizer = Optimizer::new(optimizer_settings);

    Context::<DummyDependency>::new(llvm, module, optimizer, None, true, None, None)
}

#[test]
pub fn check_attribute_null_pointer_is_invalid() {
    let llvm = inkwell::context::Context::create();
    let mut context = create_context(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
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
    let llvm = inkwell::context::Context::create();
    let mut context = create_context(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
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
    let llvm = inkwell::context::Context::create();
    let mut context = create_context(&llvm, OptimizerSettings::size());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
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
    let llvm = inkwell::context::Context::create();
    let mut context = create_context(&llvm, OptimizerSettings::cycles());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
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
    let llvm = inkwell::context::Context::create();
    let mut context = create_context(&llvm, OptimizerSettings::size());

    let function = context
        .add_function(
            "test",
            context
                .word_type()
                .fn_type(&[context.word_type().into()], false),
            1,
            Some(inkwell::module::Linkage::External),
        )
        .expect("Failed to add function");
    assert!(function
        .borrow()
        .declaration()
        .value
        .attributes(inkwell::attributes::AttributeLoc::Function)
        .contains(&llvm.create_enum_attribute(Attribute::MinSize as u32, 0)));
}
