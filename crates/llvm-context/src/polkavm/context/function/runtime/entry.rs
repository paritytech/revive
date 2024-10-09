//! The entry function.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::Context;
use crate::polkavm::r#const::*;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

/// The entry function.
/// The function is a wrapper managing the runtime and deploy code calling logic.
/// Is a special runtime function that is only used by the front-end generated code.
#[derive(Debug, Default)]
pub struct Entry {}

impl Entry {
    /// The call flags argument index.
    pub const ARGUMENT_INDEX_CALL_FLAGS: usize = 0;

    /// The number of mandatory arguments.
    pub const MANDATORY_ARGUMENTS_COUNT: usize = 2;

    /// Reserve 1kb for calldata.
    pub const MAX_CALLDATA_SIZE: usize = 1024;

    /// Initializes the global variables.
    /// The pointers are not initialized, because it's not possible to create a null pointer.
    pub fn initialize_globals<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        let calldata_type = context.array_type(context.byte_type(), Self::MAX_CALLDATA_SIZE);
        context.set_global(
            crate::polkavm::GLOBAL_CALLDATA_POINTER,
            calldata_type,
            AddressSpace::Stack,
            calldata_type.get_undef(),
        );

        context.set_global(
            crate::polkavm::GLOBAL_HEAP_MEMORY_POINTER,
            context.llvm().ptr_type(AddressSpace::Heap.into()),
            AddressSpace::Stack,
            context.xlen_type().get_undef(),
        );
        context.build_store(
            context
                .get_global(crate::polkavm::GLOBAL_HEAP_MEMORY_POINTER)?
                .into(),
            context.build_sbrk(context.integer_const(crate::polkavm::XLEN, 0))?,
        )?;

        context.set_global(
            crate::polkavm::GLOBAL_CALLDATA_SIZE,
            context.word_type(),
            AddressSpace::Stack,
            context.word_undef(),
        );

        context.set_global(
            crate::polkavm::GLOBAL_CALL_FLAGS,
            context.word_type(),
            AddressSpace::Stack,
            context.word_const(0),
        );

        Ok(())
    }

    /// Load the calldata via seal `input` and initialize the calldata end
    /// and calldata size globals.
    pub fn load_calldata<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        let input_pointer = context
            .get_global(crate::polkavm::GLOBAL_CALLDATA_POINTER)?
            .value
            .as_pointer_value();
        let input_pointer_casted = context.builder.build_ptr_to_int(
            input_pointer,
            context.xlen_type(),
            "input_pointer_casted",
        )?;

        let length_pointer = context.build_alloca_at_entry(context.xlen_type(), "len_ptr");
        let length_pointer_casted = context.builder.build_ptr_to_int(
            length_pointer.value,
            context.xlen_type(),
            "length_pointer_casted",
        )?;

        context.build_store(
            length_pointer,
            context.integer_const(crate::polkavm::XLEN, Self::MAX_CALLDATA_SIZE as u64),
        )?;
        context.build_runtime_call(
            runtime_api::imports::INPUT,
            &[input_pointer_casted.into(), length_pointer_casted.into()],
        );

        // Store the calldata size
        let calldata_size = context
            .build_load(length_pointer, "input_size")?
            .into_int_value();
        let calldata_size_casted = context.builder().build_int_z_extend(
            calldata_size,
            context.word_type(),
            "zext_input_len",
        )?;
        context.set_global(
            crate::polkavm::GLOBAL_CALLDATA_SIZE,
            context.word_type(),
            AddressSpace::Stack,
            calldata_size_casted,
        );

        Ok(())
    }

    /// Calls the deploy code if the first function argument was `1`.
    /// Calls the runtime code otherwise.
    pub fn leave_entry<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        let is_deploy = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_CALL_FLAGS);

        context.set_global(
            crate::polkavm::GLOBAL_CALL_FLAGS,
            is_deploy.get_type(),
            AddressSpace::Stack,
            is_deploy.into_int_value(),
        );

        let deploy_code_call_block = context.append_basic_block("deploy_code_call_block");
        let runtime_code_call_block = context.append_basic_block("runtime_code_call_block");

        context.build_conditional_branch(
            is_deploy.into_int_value(),
            deploy_code_call_block,
            runtime_code_call_block,
        )?;

        let deploy_code = context
            .functions
            .get(runtime::FUNCTION_DEPLOY_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract deploy code not found"))?;
        let runtime_code = context
            .functions
            .get(runtime::FUNCTION_RUNTIME_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract runtime code not found"))?;

        context.set_basic_block(deploy_code_call_block);
        context.build_call(deploy_code.borrow().declaration, &[], "deploy_code_call");
        context.build_unconditional_branch(context.current_function().borrow().return_block());

        context.set_basic_block(runtime_code_call_block);
        context.build_call(runtime_code.borrow().declaration, &[], "runtime_code_call");
        context.build_unconditional_branch(context.current_function().borrow().return_block());

        Ok(())
    }
}

impl<D> WriteLLVM<D> for Entry
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        let entry_arguments = vec![context.bool_type().as_basic_type_enum()];
        let entry_function_type = context.function_type(entry_arguments, 0, false);
        context.add_function(
            runtime::FUNCTION_ENTRY,
            entry_function_type,
            0,
            Some(inkwell::module::Linkage::External),
        )?;

        Ok(())
    }

    /// Instead of a single entrypoint, the runtime expects two exports: `call ` and `deploy`.
    /// `call` and `deploy` directly call `entry`, signaling a deploy if the first arg is `1`.
    /// The `entry` function loads calldata, sets globals and calls the runtime or deploy code.
    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        let entry = context
            .get_function(runtime::FUNCTION_ENTRY)
            .expect("the entry function should already be declared")
            .borrow()
            .declaration;
        crate::PolkaVMFunction::set_attributes(
            context.llvm(),
            entry,
            vec![crate::PolkaVMAttribute::NoReturn],
            true,
        );

        context.set_current_function(runtime::FUNCTION_ENTRY)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        Self::initialize_globals(context)?;
        Self::load_calldata(context)?;
        Self::leave_entry(context)?;

        context.build_unconditional_branch(context.current_function().borrow().return_block());
        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_unreachable();

        Ok(())
    }
}
