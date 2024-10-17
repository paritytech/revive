//! The immutable data runtime function.

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::function::runtime;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

use inkwell::debug_info::AsDIScope;

/// A function for requesting the immutable data from the runtime.
/// This is a special function that is only used by the front-end generated code.
///
/// The runtime API is called lazily and subsequent calls are no-ops.
///
/// The bytes written is asserted to match the expected length.
/// This should never fail; the length is known.
/// However, this is a one time assertion, hence worth it.
#[derive(Debug)]
pub struct ImmutableDataLoad;

impl<D> WriteLLVM<D> for ImmutableDataLoad
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.add_function(
            runtime::FUNCTION_LOAD_IMMUTABLE_DATA,
            context.void_type().fn_type(Default::default(), false),
            0,
            Some(inkwell::module::Linkage::Private),
        )?;

        Ok(())
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(runtime::FUNCTION_LOAD_IMMUTABLE_DATA, None)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        if context.debug_info().is_some() {
            context.builder().unset_current_debug_location();
            let func_scope = context
                .set_current_function_debug_info(runtime::FUNCTION_LOAD_IMMUTABLE_DATA, 0)?
                .as_debug_info_scope();
            context.push_debug_scope(func_scope);
        }

        let immutable_data_size_pointer = context
            .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_SIZE)?
            .value
            .as_pointer_value();
        let immutable_data_size = context.build_load(
            Pointer::new(
                context.xlen_type(),
                AddressSpace::Stack,
                immutable_data_size_pointer,
            ),
            "immutable_data_size_load",
        )?;

        let load_immutable_data_block = context.append_basic_block("load_immutables_block");
        let return_block = context.current_function().borrow().return_block();
        let immutable_data_size_is_zero = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            context.xlen_type().const_zero(),
            immutable_data_size.into_int_value(),
            "immutable_data_size_is_zero",
        )?;
        context.build_conditional_branch(
            immutable_data_size_is_zero,
            return_block,
            load_immutable_data_block,
        )?;

        context.set_basic_block(load_immutable_data_block);
        let output_pointer = context
            .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
            .value
            .as_pointer_value();
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::GET_IMMUTABLE_DATA,
            &[
                context
                    .builder()
                    .build_ptr_to_int(output_pointer, context.xlen_type(), "ptr_to_xlen")?
                    .into(),
                context
                    .builder()
                    .build_ptr_to_int(
                        immutable_data_size_pointer,
                        context.xlen_type(),
                        "ptr_to_xlen",
                    )?
                    .into(),
            ],
        );
        let bytes_written = context.builder().build_load(
            context.xlen_type(),
            immutable_data_size_pointer,
            "bytes_written",
        )?;
        context.builder().build_store(
            immutable_data_size_pointer,
            context.xlen_type().const_zero(),
        )?;
        let overflow_block = context.append_basic_block("immutable_data_overflow");
        let is_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            immutable_data_size.into_int_value(),
            bytes_written.into_int_value(),
            "is_overflow",
        )?;
        context.build_conditional_branch(is_overflow, overflow_block, return_block)?;

        context.set_basic_block(overflow_block);
        context.build_call(context.intrinsics().trap, &[], "invalid_trap");
        context.build_unreachable();

        context.set_basic_block(return_block);
        context.build_return(None);

        context.pop_debug_scope();

        Ok(())
    }
}
