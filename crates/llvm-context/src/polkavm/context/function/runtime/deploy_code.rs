//! The deploy code function.

use std::marker::PhantomData;

use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

use inkwell::debug_info::AsDIScope;

/// The deploy code function.
/// Is a special function that is only used by the front-end generated code.
#[derive(Debug)]
pub struct DeployCode<B, D>
where
    B: WriteLLVM<D>,
    D: Dependency + Clone,
{
    /// The deploy code AST representation.
    inner: B,
    /// The `D` phantom data.
    _pd: PhantomData<D>,
}

impl<B, D> DeployCode<B, D>
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

impl<B, D> WriteLLVM<D> for DeployCode<B, D>
where
    B: WriteLLVM<D>,
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        let function_type =
            context.function_type::<inkwell::types::BasicTypeEnum>(vec![], 0, false);
        context.add_function(
            runtime::FUNCTION_DEPLOY_CODE,
            function_type,
            0,
            Some(inkwell::module::Linkage::External),
        )?;

        self.inner.declare(context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(runtime::FUNCTION_DEPLOY_CODE)?;

        context.set_basic_block(context.current_function().borrow().entry_block());
        context.set_code_type(CodeType::Deploy);

        if let Some(dinfo) = context.debug_info() {
            context.builder().unset_current_debug_location();
            let di_builder = dinfo.builder();
            let line_num: u32 = 0;
            let column: u32 = 0;
            let func_name: &str = runtime::FUNCTION_DEPLOY_CODE;
            let linkage_name = dinfo.namespace_as_identifier(Some(func_name).clone());
            let di_file = dinfo.compilation_unit().get_file();
            let di_scope = di_file.as_debug_info_scope();
            let di_func_scope = dinfo.create_function(
                di_scope,
                func_name,
                Some(linkage_name.as_str()),
                None,
                &[],
                di_file,
                line_num,
                true,
                false,
                false,
                Some(inkwell::debug_info::DIFlagsConstants::PUBLIC),
            )?;
            let func_value = context
                .current_function()
                .borrow()
                .declaration()
                .function_value();
            let _ = func_value.set_subprogram(di_func_scope);

            let lexical_scope = di_builder
                .create_lexical_block(
                    di_func_scope.as_debug_info_scope(),
                    dinfo.compilation_unit().get_file(),
                    line_num,
                    column,
                )
                .as_debug_info_scope();
            let _ = dinfo.push_scope(lexical_scope);
        }

        self.inner.into_llvm(context)?;
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
        if let Some(dinfo) = context.debug_info() {
            context.builder().unset_current_debug_location();
            let di_builder = dinfo.builder();
            let line_num: u32 = 0x0beef;
            let di_parent_scope = dinfo
                .top_scope()
                .expect("expected an existing debug-info scope")
                .clone();
            let di_loc = di_builder.create_debug_location(
                context.llvm(),
                line_num,
                0,
                di_parent_scope,
                None,
            );
            context.builder().set_current_debug_location(di_loc)
        }
        context.build_return(None);

        if let Some(dinfo) = context.debug_info() {
            let _ = dinfo.pop_scope();
        }

        Ok(())
    }
}
