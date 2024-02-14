//!
//! The entry function.
//!

use inkwell::types::BasicType;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::function::runtime::Runtime;
use crate::eravm::context::Context;
use crate::eravm::Dependency;
use crate::eravm::WriteLLVM;
use crate::EraVMPointer as Pointer;

///
/// The entry function.
///
/// The function is a wrapper managing the runtime and deploy code calling logic.
///
/// Is a special runtime function that is only used by the front-end generated code.
///
#[derive(Debug, Default)]
pub struct Entry {}

impl Entry {
    /// The call flags argument index.
    pub const ARGUMENT_INDEX_CALL_FLAGS: usize = 0;

    /// The number of mandatory arguments.
    pub const MANDATORY_ARGUMENTS_COUNT: usize = 2;

    /// Initializes the global variables.
    /// The pointers are not initialized, because it's not possible to create a null pointer.
    pub fn initialize_globals<D>(context: &mut Context<D>)
    where
        D: Dependency + Clone,
    {
        let calldata_type = context.array_type(context.byte_type(), 1024 * 1024);
        context.set_global(
            crate::eravm::GLOBAL_CALLDATA,
            calldata_type,
            AddressSpace::Stack,
            calldata_type.get_undef(),
        );

        context.set_global(
            crate::eravm::GLOBAL_HEAP_MEMORY_POINTER,
            context.field_type(),
            AddressSpace::Stack,
            context.field_const(0),
        );

        context.set_global(
            crate::eravm::GLOBAL_CALLDATA_SIZE,
            context.field_type(),
            AddressSpace::Stack,
            context.field_undef(),
        );
        context.set_global(
            crate::eravm::GLOBAL_RETURN_DATA_SIZE,
            context.field_type(),
            AddressSpace::Stack,
            context.field_const(0),
        );

        context.set_global(
            crate::eravm::GLOBAL_CALL_FLAGS,
            context.field_type(),
            AddressSpace::Stack,
            context.field_const(0),
        );

        let extra_abi_data_type = context.array_type(
            context.field_type().as_basic_type_enum(),
            crate::eravm::EXTRA_ABI_DATA_SIZE,
        );
        context.set_global(
            crate::eravm::GLOBAL_EXTRA_ABI_DATA,
            extra_abi_data_type,
            AddressSpace::Stack,
            extra_abi_data_type.const_zero(),
        );
    }

    /// Load the calldata via seal `input` and initialize the calldata, calldata end
    /// and calldata size globals.
    pub fn load_calldata<D>(context: &mut Context<D>) -> anyhow::Result<()>
    where
        D: Dependency + Clone,
    {
        // Call the input runtime API
        let input_function = context
            .module()
            .get_function("input")
            .expect("should be declared");
        let input_pointer = context
            .get_global(crate::eravm::GLOBAL_CALLDATA)
            .unwrap()
            .value
            .as_pointer_value();
        let input_pointer_casted = context.builder.build_ptr_to_int(
            input_pointer,
            context.integer_type(32),
            format!("arg_pointer_casted").as_str(),
        )?;
        let input_length_pointer = context.build_alloca(context.integer_type(32), "len_ptr");
        let length_pointer_casted = context.builder.build_ptr_to_int(
            input_length_pointer.value,
            context.integer_type(32),
            format!("arg_pointer_casted").as_str(),
        )?;
        let arguments = &[input_pointer_casted.into(), length_pointer_casted.into()];
        context
            .builder()
            .build_call(input_function, arguments, "call_seal_input")?;

        // Store calldata pointer
        let input_pointer = Pointer::new(
            input_pointer.get_type(),
            AddressSpace::Generic,
            input_pointer,
        );
        context.write_abi_pointer(input_pointer, crate::eravm::GLOBAL_CALLDATA_POINTER);

        // Store the calldata size
        let calldata_size_casted = context.builder().build_int_z_extend(
            context
                .build_load(input_length_pointer, "input_size")
                .into_int_value(),
            context.field_type(),
            "zext_input_len",
        )?;
        context.set_global(
            crate::eravm::GLOBAL_CALLDATA_SIZE,
            context.field_type(),
            AddressSpace::Stack,
            calldata_size_casted,
        );

        // Store calldata end pointer
        let calldata_end_pointer = context.build_gep(
            input_pointer,
            &[calldata_size_casted],
            context
                .byte_type()
                .ptr_type(AddressSpace::Generic.into())
                .as_basic_type_enum(),
            "return_data_abi_initializer",
        );
        context.write_abi_pointer(
            calldata_end_pointer,
            crate::eravm::GLOBAL_RETURN_DATA_POINTER,
        );
        context.write_abi_pointer(calldata_end_pointer, crate::eravm::GLOBAL_ACTIVE_POINTER);

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
            crate::eravm::GLOBAL_CALL_FLAGS,
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
            .get(Runtime::FUNCTION_DEPLOY_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract deploy code not found"))?;
        let runtime_code = context
            .functions
            .get(Runtime::FUNCTION_RUNTIME_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract runtime code not found"))?;

        context.set_basic_block(deploy_code_call_block);
        context.build_invoke(deploy_code.borrow().declaration, &[], "deploy_code_call");
        context.build_unconditional_branch(context.current_function().borrow().return_block());

        context.set_basic_block(runtime_code_call_block);
        context.build_invoke(runtime_code.borrow().declaration, &[], "runtime_code_call");
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
        context.add_function(Runtime::FUNCTION_ENTRY, entry_function_type, 0, None)?;

        context.declare_extern_function("deploy")?;
        context.declare_extern_function("call")?;

        Ok(())
    }

    /// `call` and `deploy` call `entry`, signaling deploy if the first arg is `1`.
    /// `entry` loads calldata, sets globals and calls the runtime or deploy code.
    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        let entry = context
            .get_function(Runtime::FUNCTION_ENTRY)
            .expect("the entry function should already be declared")
            .borrow()
            .declaration;

        context.set_current_function("deploy")?;
        context.set_basic_block(context.current_function().borrow().entry_block());
        assert!(context
            .build_invoke(entry, &[context.bool_const(true).into()], "entry_deploy")
            .is_none());
        context.set_basic_block(context.current_function().borrow().return_block);
        context.build_return(None);

        context.set_current_function("call")?;
        context.set_basic_block(context.current_function().borrow().entry_block());
        assert!(context
            .build_invoke(entry, &[context.bool_const(false).into()], "entry_call")
            .is_none());
        context.set_basic_block(context.current_function().borrow().return_block);
        context.build_return(None);

        context.set_current_function(Runtime::FUNCTION_ENTRY)?;
        context.set_basic_block(context.current_function().borrow().entry_block());
        Self::initialize_globals(context);
        Self::load_calldata(context)?;
        Self::leave_entry(context)?;
        context.build_unconditional_branch(context.current_function().borrow().return_block());
        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_return(None);

        /*
        let deploy_code_call_block = context.append_basic_block("deploy_code_call_block");
        let runtime_code_call_block = context.append_basic_block("runtime_code_call_block");

        let deploy_code = context
            .functions
            .get(Runtime::FUNCTION_DEPLOY_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract deploy code not found"))?;
        let runtime_code = context
            .functions
            .get(Runtime::FUNCTION_RUNTIME_CODE)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Contract runtime code not found"))?;

        context.set_basic_block(context.current_function().borrow().entry_block());
        Self::initialize_globals(context)?;

        let calldata_abi = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_CALLDATA_ABI);
        let calldata_abi_pointer = Pointer::new(
            context.byte_type(),
            AddressSpace::Generic,
            calldata_abi.into_pointer_value(),
        );
        context.write_abi_pointer(calldata_abi_pointer, crate::eravm::GLOBAL_CALLDATA_POINTER);
        context.write_abi_data_size(calldata_abi_pointer, crate::eravm::GLOBAL_CALLDATA_SIZE);
        let calldata_length = context.get_global_value(crate::eravm::GLOBAL_CALLDATA_SIZE)?;
        let calldata_end_pointer = context.build_gep(
            calldata_abi_pointer,
            &[calldata_length.into_int_value()],
            context
                .byte_type()
                .ptr_type(AddressSpace::Generic.into())
                .as_basic_type_enum(),
            "return_data_abi_initializer",
        );
        context.write_abi_pointer(
            calldata_end_pointer,
            crate::eravm::GLOBAL_RETURN_DATA_POINTER,
        );
        context.write_abi_pointer(calldata_end_pointer, crate::eravm::GLOBAL_ACTIVE_POINTER);

        let call_flags = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_CALL_FLAGS);
        context.set_global(
            crate::eravm::GLOBAL_CALL_FLAGS,
            call_flags.get_type(),
            AddressSpace::Stack,
            call_flags.into_int_value(),
        );

        let extra_abi_data_global = context.get_global(crate::eravm::GLOBAL_EXTRA_ABI_DATA)?;
        for (array_index, argument_index) in (Self::MANDATORY_ARGUMENTS_COUNT
            ..Self::MANDATORY_ARGUMENTS_COUNT + crate::eravm::EXTRA_ABI_DATA_SIZE)
            .enumerate()
        {
            let array_element_pointer = context.build_gep(
                extra_abi_data_global.into(),
                &[
                    context.field_const(0),
                    context
                        .integer_type(era_compiler_common::BIT_LENGTH_X32)
                        .const_int(array_index as u64, false),
                ],
                context.field_type().as_basic_type_enum(),
                "extra_abi_data_array_element_pointer",
            );
            let argument_value = context
                .current_function()
                .borrow()
                .get_nth_param(argument_index)
                .into_int_value();
            context.build_store(array_element_pointer, argument_value);
        }

        let is_deploy_call_flag_truncated = context.builder().build_and(
            call_flags.into_int_value(),
            context.field_const(1),
            "is_deploy_code_call_flag_truncated",
        )?;
        let is_deploy_code_call_flag = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            is_deploy_call_flag_truncated,
            context.field_const(1),
            "is_deploy_code_call_flag",
        )?;
        context.build_conditional_branch(
            is_deploy_code_call_flag,
            deploy_code_call_block,
            runtime_code_call_block,
        );

        gontext.set_basic_block(deploy_code_call_block);
        context.build_invoke(deploy_code.borrow().declaration, &[], "deploy_code_call");
        context.build_unconditional_branch(context.current_function().borrow().return_block());

        context.set_basic_block(runtime_code_call_block);
        context.build_invoke(runtime_code.borrow().declaration, &[], "runtime_code_call");
        context.build_unconditional_branch(context.current_function().borrow().return_block());
        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_return(Some(&context.field_const(0)));
        */

        Ok(())
    }
}
