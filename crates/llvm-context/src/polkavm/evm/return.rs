//! Translates the transaction return operations.

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::pointer::Pointer;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `return` instruction.
pub fn r#return<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    match context.code_type() {
        None => anyhow::bail!("Return is not available if the contract part is undefined"),
        Some(CodeType::Deploy) => {
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
        }
        Some(CodeType::Runtime) => {}
    }

    context.build_exit(
        context.integer_const(crate::polkavm::XLEN, 0),
        offset,
        length,
    )
}

/// Translates the `revert` instruction.
pub fn revert<'ctx, D>(
    context: &mut Context<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    context.build_exit(
        context.integer_const(crate::polkavm::XLEN, 1),
        offset,
        length,
    )
}

/// Translates the `stop` instruction.
/// Is the same as `return(0, 0)`.
pub fn stop<D>(context: &mut Context<D>) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    r#return(context, context.word_const(0), context.word_const(0))
}

/// Translates the `invalid` instruction.
/// Burns all gas using an out-of-bounds memory store, causing a panic.
pub fn invalid<D>(context: &mut Context<D>) -> anyhow::Result<()>
where
    D: Dependency + Clone,
{
    crate::polkavm::evm::memory::store(
        context,
        context.word_type().const_all_ones(),
        context.word_const(0),
    )?;
    context.build_call(context.intrinsics().trap, &[], "invalid_trap");
    Ok(())
}
