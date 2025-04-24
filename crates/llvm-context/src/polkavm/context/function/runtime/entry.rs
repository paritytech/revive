//! The entry function.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::Context;
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

    /// Initializes the global variables.
    /// The pointers are not initialized, because it's not possible to create a null pointer.
    pub fn initialize_globals<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        context.set_global(
            crate::polkavm::GLOBAL_CALLDATA_SIZE,
            context.xlen_type(),
            AddressSpace::Stack,
            context.xlen_type().get_undef(),
        );

        context.set_global(
            crate::polkavm::GLOBAL_HEAP_SIZE,
            context.xlen_type(),
            AddressSpace::Stack,
            context.xlen_type().const_zero(),
        );

        let heap_memory_type = context
            .byte_type()
            .array_type(context.memory_config.heap_size);
        context.set_global(
            crate::polkavm::GLOBAL_HEAP_MEMORY,
            heap_memory_type,
            AddressSpace::Stack,
            heap_memory_type.const_zero(),
        );

        let address_type = context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
        context.set_global(
            crate::polkavm::GLOBAL_ADDRESS_SPILL_BUFFER,
            address_type,
            AddressSpace::Stack,
            address_type.const_zero(),
        );

        Ok(())
    }

    /// Populate the calldata size global value.
    pub fn load_calldata_size<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        let call_data_size_pointer = context
            .get_global(crate::polkavm::GLOBAL_CALLDATA_SIZE)?
            .value
            .as_pointer_value();
        let call_data_size_value = context
            .build_runtime_call(revive_runtime_api::polkavm_imports::CALL_DATA_SIZE, &[])
            .expect("the call_data_size syscall method should return a value")
            .into_int_value();
        let call_data_size_value = context.builder().build_int_truncate(
            call_data_size_value,
            context.xlen_type(),
            "call_data_size_truncated",
        )?;
        context
            .builder()
            .build_store(call_data_size_pointer, call_data_size_value)?;

        Ok(())
    }

    /// Calls the deploy code if the first function argument was `1`.
    /// Calls the runtime code otherwise.
    pub fn leave_entry<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        context.set_debug_location(0, 0, None)?;

        let is_deploy = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_CALL_FLAGS);

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
        let entry_function_type = context.function_type(entry_arguments, 0);
        context.add_function(
            runtime::FUNCTION_ENTRY,
            entry_function_type,
            0,
            Some(inkwell::module::Linkage::External),
        )?;

        context.declare_global(
            revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER,
            context.word_type().array_type(0),
            AddressSpace::Stack,
        );

        context.declare_global(
            revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_SIZE,
            context.xlen_type(),
            AddressSpace::Stack,
        );

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
            &[crate::PolkaVMAttribute::NoReturn],
            true,
        );

        context.set_current_function(runtime::FUNCTION_ENTRY, None)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        Self::initialize_globals(context)?;
        Self::load_calldata_size(context)?;
        Self::leave_entry(context)?;

        context.build_unconditional_branch(context.current_function().borrow().return_block());
        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_unreachable();

        context.pop_debug_scope();

        Ok(())
    }
}
