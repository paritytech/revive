//!
//! The `default_call` function.
//!

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::eravm::context::function::llvm_runtime::LLVMRuntime;
use crate::eravm::context::function::Function;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;
use crate::eravm::WriteLLVM;

///
/// The `default_call` function.
///
/// Generates a default contract call, if the `msg.value` is zero.
///
#[derive(Debug)]
pub struct DefaultCall {
    /// The name of the inner function used for the low-level call.
    inner_name: String,
    /// The function name with the low-level function name as an element.
    name: String,
}

impl DefaultCall {
    /// The gas argument index.
    pub const ARGUMENT_INDEX_GAS: usize = 0;

    /// The address argument index.
    pub const ARGUMENT_INDEX_ADDRESS: usize = 1;

    /// The input offset argument index.
    pub const ARGUMENT_INDEX_INPUT_OFFSET: usize = 2;

    /// The input length argument index.
    pub const ARGUMENT_INDEX_INPUT_LENGTH: usize = 3;

    /// The output offset argument index.
    pub const ARGUMENT_INDEX_OUTPUT_OFFSET: usize = 4;

    /// The output length argument index.
    pub const ARGUMENT_INDEX_OUTPUT_LENGTH: usize = 5;

    ///
    /// A shortcut constructor.
    ///
    pub fn new(call_function: FunctionDeclaration) -> Self {
        let inner_name = call_function.value.get_name().to_string_lossy().to_string();
        let name = Self::name(call_function);

        Self { inner_name, name }
    }

    ///
    /// Returns the function name.
    ///
    pub fn name(call_function: FunctionDeclaration) -> String {
        let suffix = match call_function.value.get_name().to_string_lossy() {
            name if name == LLVMRuntime::FUNCTION_FARCALL => "far",
            name if name == LLVMRuntime::FUNCTION_STATICCALL => "static",
            name if name == LLVMRuntime::FUNCTION_DELEGATECALL => "delegate",
            name => panic!("Invalid low-level call inner function `{name}`"),
        };
        format!("__default_{suffix}_call")
    }

    ///
    /// Returns the low-level call function.
    ///
    fn inner_function<'ctx, D>(&self, context: &Context<'ctx, D>) -> FunctionDeclaration<'ctx>
    where
        D: Dependency + Clone,
    {
        match self.inner_name.as_str() {
            name if name == LLVMRuntime::FUNCTION_FARCALL => context.llvm_runtime().far_call,
            name if name == LLVMRuntime::FUNCTION_STATICCALL => context.llvm_runtime().static_call,
            name if name == LLVMRuntime::FUNCTION_DELEGATECALL => {
                context.llvm_runtime().delegate_call
            }
            name => panic!("Invalid low-level call inner function `{name}`"),
        }
    }
}

impl<D> WriteLLVM<D> for DefaultCall
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        let function_type = context.function_type(
            vec![
                context.field_type().as_basic_type_enum(),
                context.field_type().as_basic_type_enum(),
                context.field_type().as_basic_type_enum(),
                context.field_type().as_basic_type_enum(),
                context.field_type().as_basic_type_enum(),
                context.field_type().as_basic_type_enum(),
            ],
            1,
            false,
        );
        let function = context.add_function(
            self.name.as_str(),
            function_type,
            1,
            Some(inkwell::module::Linkage::Private),
        )?;
        Function::set_frontend_runtime_attributes(
            context.llvm,
            function.borrow().declaration(),
            &context.optimizer,
        );

        Ok(())
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(self.name.as_str())?;

        let gas = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_GAS)
            .into_int_value();
        let address = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_ADDRESS)
            .into_int_value();
        let input_offset = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_INPUT_OFFSET)
            .into_int_value();
        let input_length = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_INPUT_LENGTH)
            .into_int_value();
        let output_offset = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_OUTPUT_OFFSET)
            .into_int_value();
        let output_length = context
            .current_function()
            .borrow()
            .get_nth_param(Self::ARGUMENT_INDEX_OUTPUT_LENGTH)
            .into_int_value();

        context.set_basic_block(context.current_function().borrow().entry_block());
        let status_code_result_pointer = context.build_alloca(
            context.field_type(),
            "contract_call_result_status_code_pointer",
        );
        context.build_store(status_code_result_pointer, context.field_const(0));

        let abi_data = crate::eravm::utils::abi_data(
            context,
            input_offset,
            input_length,
            Some(gas),
            AddressSpace::Heap,
            false,
        )?
        .into_int_value();

        let result = context
            .build_call(
                self.inner_function(context),
                crate::eravm::utils::external_call_arguments(
                    context,
                    abi_data.as_basic_value_enum(),
                    address,
                    vec![],
                    None,
                )
                .as_slice(),
                "contract_call_external",
            )
            .expect("IntrinsicFunction always returns a flag");

        let result_abi_data = context
            .builder()
            .build_extract_value(
                result.into_struct_value(),
                0,
                "contract_call_external_result_abi_data",
            )
            .expect("Always exists");
        let result_abi_data_pointer = Pointer::new(
            context.byte_type(),
            AddressSpace::Generic,
            result_abi_data.into_pointer_value(),
        );
        let result_abi_data_casted = result_abi_data_pointer.cast(context.field_type());

        let result_status_code_boolean = context
            .builder()
            .build_extract_value(
                result.into_struct_value(),
                1,
                "contract_call_external_result_status_code_boolean",
            )
            .expect("Always exists");
        let result_status_code = context.builder().build_int_z_extend_or_bit_cast(
            result_status_code_boolean.into_int_value(),
            context.field_type(),
            "contract_call_external_result_status_code",
        )?;
        context.build_store(status_code_result_pointer, result_status_code);

        let source = result_abi_data_casted;

        let destination = Pointer::new_with_offset(
            context,
            AddressSpace::Heap,
            context.byte_type(),
            output_offset,
            "contract_call_destination",
        );

        context.build_memcpy_return_data(
            context.intrinsics().memory_copy_from_generic,
            destination,
            source,
            output_length,
            "contract_call_memcpy_from_child",
        );

        context.write_abi_pointer(
            result_abi_data_pointer,
            crate::eravm::GLOBAL_RETURN_DATA_POINTER,
        );
        context.write_abi_data_size(
            result_abi_data_pointer,
            crate::eravm::GLOBAL_RETURN_DATA_SIZE,
        );
        context.build_unconditional_branch(context.current_function().borrow().return_block());

        context.set_basic_block(context.current_function().borrow().return_block());
        let status_code_result =
            context.build_load(status_code_result_pointer, "contract_call_status_code");
        context.build_return(Some(&status_code_result));

        Ok(())
    }
}
