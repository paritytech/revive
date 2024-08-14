//! The runtime code function.

use std::marker::PhantomData;

use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

use inkwell::debug_info::AsDIScope;

/// The runtime code function.
/// Is a special function that is only used by the front-end generated code.
#[derive(Debug)]
pub struct RuntimeCode<B, D>
where
    B: WriteLLVM<D>,
    D: Dependency + Clone,
{
    /// The runtime code AST representation.
    inner: B,
    /// The `D` phantom data.
    _pd: PhantomData<D>,
}

impl<B, D> RuntimeCode<B, D>
where
    B: WriteLLVM<D>,
    D: Dependency + Clone,
{
    /// A shortcut constructor.
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            _pd: PhantomData,
        }
    }
}

impl<B, D> WriteLLVM<D> for RuntimeCode<B, D>
where
    B: WriteLLVM<D>,
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        let function_type = context.function_type::<inkwell::types::BasicTypeEnum>(vec![], 0);
        context.add_function(
            runtime::FUNCTION_RUNTIME_CODE,
            function_type,
            0,
            Some(inkwell::module::Linkage::External),
        )?;

        self.inner.declare(context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(runtime::FUNCTION_RUNTIME_CODE)?;

        context.set_basic_block(context.current_function().borrow().entry_block());
        context.set_code_type(CodeType::Runtime);
        if context.debug_info().is_some() {
            context.builder().unset_current_debug_location();
            let func_scope = context
                .set_current_function_debug_info(runtime::FUNCTION_RUNTIME_CODE, 0)?
                .as_debug_info_scope();
            context.push_debug_scope(func_scope);
        }

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
        context.build_unreachable();

        context.pop_debug_scope();

        Ok(())
    }
}
