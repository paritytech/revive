//! Translates the contract immutable operations.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// A function for requesting the immutable data from the runtime.
/// This is a special function that is only used by the front-end generated code.
///
/// The runtime API is called lazily and subsequent calls are no-ops.
///
/// The bytes written is asserted to match the expected length.
/// This should never fail; the length is known.
/// However, this is a one time assertion, hence worth it.
pub struct Load;

impl RuntimeFunction for Load {
    const NAME: &'static str = "__revive_load_immutable_data";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(Default::default(), false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
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

        Ok(None)
    }
}

impl WriteLLVM for Load {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Store the immutable data from the constructor code.
pub struct Store;

impl RuntimeFunction for Store {
    const NAME: &'static str = "__revive_store_immutable_data";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(Default::default(), false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
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

        let write_immutable_data_block = context.append_basic_block("write_immutables_block");
        let join_return_block = context.append_basic_block("join_return_block");
        let immutable_data_size_is_zero = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            context.xlen_type().const_zero(),
            immutable_data_size.into_int_value(),
            "immutable_data_size_is_zero",
        )?;
        context.build_conditional_branch(
            immutable_data_size_is_zero,
            join_return_block,
            write_immutable_data_block,
        )?;

        context.set_basic_block(write_immutable_data_block);
        let immutable_data_pointer = context
            .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
            .value
            .as_pointer_value();
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::SET_IMMUTABLE_DATA,
            &[
                context
                    .builder()
                    .build_ptr_to_int(
                        immutable_data_pointer,
                        context.xlen_type(),
                        "immutable_data_pointer_to_xlen",
                    )?
                    .into(),
                immutable_data_size,
            ],
        );
        context.build_unconditional_branch(join_return_block);

        context.set_basic_block(join_return_block);
        Ok(None)
    }
}

impl WriteLLVM for Store {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Translates the contract immutable load.
///
/// In deploy code the values are read from the stack.
///
/// In runtime code they are loaded lazily with the `get_immutable_data` syscall.
pub fn load<'ctx>(
    context: &mut Context<'ctx>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    match context.code_type() {
        None => {
            anyhow::bail!("Immutables are not available if the contract part is undefined");
        }
        Some(CodeType::Deploy) => load_from_memory(context, index),
        Some(CodeType::Runtime) => {
            let name = <Load as RuntimeFunction>::NAME;
            context.build_call(
                context
                    .get_function(name, false)
                    .expect("is always declared for runtime code")
                    .borrow()
                    .declaration(),
                &[],
                name,
            );
            load_from_memory(context, index)
        }
    }
}

/// Translates the contract immutable store.
///
/// In deploy code the values are written to the stack at the predefined offset,
/// being prepared for storing them using the `set_immutable_data` syscall.
///
/// Ignored in the runtime code.
pub fn store<'ctx>(
    context: &mut Context<'ctx>,
    index: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    match context.code_type() {
        None => {
            anyhow::bail!("Immutables are not available if the contract part is undefined");
        }
        Some(CodeType::Deploy) => {
            let immutable_data_pointer = context
                .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
                .value
                .as_pointer_value();
            let immutable_pointer = context.build_gep(
                Pointer::new(
                    context.word_type(),
                    AddressSpace::Stack,
                    immutable_data_pointer,
                ),
                &[index],
                context.word_type().as_basic_type_enum(),
                "immutable_variable_pointer",
            );
            context.build_store(immutable_pointer, value)
        }
        Some(CodeType::Runtime) => {
            anyhow::bail!("Immutable writes are not available in the runtime code");
        }
    }
}

pub fn load_from_memory<'ctx>(
    context: &mut Context<'ctx>,
    index: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let immutable_data_pointer = context
        .get_global(revive_runtime_api::immutable_data::GLOBAL_IMMUTABLE_DATA_POINTER)?
        .value
        .as_pointer_value();
    let immutable_pointer = context.build_gep(
        Pointer::new(
            context.word_type(),
            AddressSpace::Stack,
            immutable_data_pointer,
        ),
        &[index],
        context.word_type().as_basic_type_enum(),
        "immutable_variable_pointer",
    );
    context.build_load(immutable_pointer, "immutable_value")
}
