//! Translates the transaction return operations.

use crate::polkavm::context::code_type::CodeType;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::evm::immutable::Store;

/// Translates the `return` instruction.
pub fn r#return<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    match context.code_type() {
        None => anyhow::bail!("Return is not available if the contract part is undefined"),
        Some(CodeType::Deploy) => {
            context.build_call(
                <Store as RuntimeFunction>::declaration(context),
                Default::default(),
                "store_immutable_data",
            );
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
pub fn revert<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    context.build_exit(
        context.integer_const(crate::polkavm::XLEN, 1),
        offset,
        length,
    )
}

/// Translates the `stop` instruction.
/// Is the same as `return(0, 0)`.
pub fn stop(context: &mut Context) -> anyhow::Result<()> {
    r#return(context, context.word_const(0), context.word_const(0))
}

/// Translates the `invalid` instruction.
/// Burns all gas using an out-of-bounds memory store, causing a panic.
pub fn invalid(context: &mut Context) -> anyhow::Result<()> {
    let invalid_block = context.append_basic_block("explicit_invalid");
    context.build_unconditional_branch(invalid_block);
    context.set_basic_block(invalid_block);
    context.build_runtime_call(revive_runtime_api::polkavm_imports::INVALID, &[]);
    context.build_unreachable();

    context.set_basic_block(context.append_basic_block("dead_code"));

    Ok(())
}

/// Translates the `selfdestruct` instruction.
pub fn selfdestruct<'ctx>(
    context: &mut Context<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let address_pointer = context.build_address_argument_store(address)?;
    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::TERMINATE,
        &[address_pointer.to_int(context).into()],
    );
    Ok(())
}
