//! The deploy code function.

use std::marker::PhantomData;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::function::runtime::Runtime;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

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
            Runtime::FUNCTION_DEPLOY_CODE,
            function_type,
            0,
            Some(inkwell::module::Linkage::External),
        )?;

        self.inner.declare(context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(Runtime::FUNCTION_DEPLOY_CODE)?;

        context.set_basic_block(context.current_function().borrow().entry_block());
        context.set_code_type(CodeType::Deploy);
        if let Some(vyper) = context.vyper_data.as_ref() {
            for index in 0..vyper.immutables_size() / revive_common::BYTE_LENGTH_WORD {
                let offset = (crate::polkavm::r#const::HEAP_AUX_OFFSET_CONSTRUCTOR_RETURN_DATA
                    as usize)
                    + (1 + index) * 2 * revive_common::BYTE_LENGTH_WORD;
                let value = index * revive_common::BYTE_LENGTH_WORD;
                let pointer = Pointer::new_with_offset(
                    context,
                    AddressSpace::HeapAuxiliary,
                    context.word_type(),
                    context.word_const(offset as u64),
                    "immutable_index_initializer",
                );
                context.build_store(pointer, context.word_const(value as u64))?;
            }
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
        context.build_return(None);

        Ok(())
    }
}
