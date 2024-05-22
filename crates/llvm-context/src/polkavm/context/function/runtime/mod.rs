//! The front-end runtime functions.

pub mod default_call;
pub mod deploy_code;
pub mod entry;
pub mod runtime_code;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

use self::default_call::DefaultCall;

/// The front-end runtime functions.
#[derive(Debug, Clone)]
pub struct Runtime {
    /// The address space where the calldata is allocated.
    /// Solidity uses the ordinary heap. Vyper uses the auxiliary heap.
    _address_space: AddressSpace,
}

impl Runtime {
    /// The main entry function name.
    pub const FUNCTION_ENTRY: &'static str = "__entry";

    /// The deploy code function name.
    pub const FUNCTION_DEPLOY_CODE: &'static str = "__deploy";

    /// The runtime code function name.
    pub const FUNCTION_RUNTIME_CODE: &'static str = "__runtime";

    /// A shortcut constructor.
    pub fn new(_address_space: AddressSpace) -> Self {
        Self { _address_space }
    }

    /// Returns the corresponding runtime function.
    pub fn default_call<'ctx, D>(
        context: &Context<'ctx, D>,
        call_function: FunctionDeclaration<'ctx>,
    ) -> FunctionDeclaration<'ctx>
    where
        D: Dependency + Clone,
    {
        context
            .get_function(DefaultCall::name(call_function).as_str())
            .expect("Always exists")
            .borrow()
            .declaration()
    }
}

impl<D> WriteLLVM<D> for Runtime
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        //DefaultCall::new(context.llvm_runtime().far_call).declare(context)?;
        DefaultCall::new(context.llvm_runtime().static_call).declare(context)?;
        DefaultCall::new(context.llvm_runtime().delegate_call).declare(context)?;

        Ok(())
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        //DefaultCall::new(context.llvm_runtime().far_call).into_llvm(context)?;
        DefaultCall::new(context.llvm_runtime().static_call).into_llvm(context)?;
        DefaultCall::new(context.llvm_runtime().delegate_call).into_llvm(context)?;

        Ok(())
    }
}
