use crate::optimizer::{settings::Settings as OptimizerSettings, Optimizer};
use crate::polkavm::context::Context;
use crate::PolkaVMTarget;

/// Creates a new LLVM context.
pub fn create_context(
    llvm: &inkwell::context::Context,
    optimizer_settings: OptimizerSettings,
) -> Context<'_> {
    crate::initialize_llvm(PolkaVMTarget::PVM, "resolc", Default::default());

    let module = llvm.create_module("test");
    let optimizer = Optimizer::new(optimizer_settings);

    Context::new(
        llvm,
        module,
        optimizer,
        Default::default(),
        Default::default(),
    )
}
