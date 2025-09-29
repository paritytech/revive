//! The deploy code function.

use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// The deploy code function.
/// Is a special function that is only used by the front-end generated code.
#[derive(Debug)]
pub struct DeployCode<B>
where
    B: WriteLLVM,
{
    /// The deploy code AST representation.
    inner: B,
}

impl<B> DeployCode<B>
where
    B: WriteLLVM,
{
    /// A shortcut constructor.
    pub fn new(inner: B) -> Self {
        Self { inner }
    }
}

impl<B> WriteLLVM for DeployCode<B>
where
    B: WriteLLVM,
{
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        let function_type = context.function_type::<inkwell::types::BasicTypeEnum>(vec![], 0);
        context.add_function(
            runtime::FUNCTION_DEPLOY_CODE,
            function_type,
            0,
            Some(inkwell::module::Linkage::Private),
            None,
        )?;

        self.inner.declare(context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        context.set_current_function(runtime::FUNCTION_DEPLOY_CODE, None)?;

        context.set_basic_block(context.current_function().borrow().entry_block());
        context.set_code_type(CodeType::Deploy);

        self.inner.into_llvm(context)?;
        context.set_debug_location(0, 0, None)?;

        match context
            .basic_block()
            .get_last_instruction()
            .map(|instruction| instruction.get_opcode())
        {
            Some(inkwell::values::InstructionOpcode::Br) => {}
            Some(inkwell::values::InstructionOpcode::Switch) => {}
            _ => context
                .build_unconditional_branch(context.current_function().borrow().return_block()),
        }

        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_return(None);

        context.pop_debug_scope();

        Ok(())
    }
}
