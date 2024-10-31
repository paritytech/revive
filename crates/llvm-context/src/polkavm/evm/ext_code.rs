//! Translates the external code operations.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Translates the `extcodesize` instruction if `address` is `Some`.
/// Otherwise, translates the `codesize` instruction.
pub fn size<'ctx, D>(
    context: &mut Context<'ctx, D>,
    address: Option<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address = match address {
        Some(address) => address,
        None => super::context::address(context)?.into_int_value(),
    };

    let address_pointer = context.build_address_argument_store(address)?;
    let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::CODE_SIZE,
        &[
            address_pointer.to_int(context).into(),
            output_pointer.to_int(context).into(),
        ],
    );

    context.build_load(output_pointer, "code_size")
}

/// Translates the `extcodehash` instruction.
pub fn hash<'ctx, D>(
    context: &mut Context<'ctx, D>,
    address: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let address_type = context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
    let address_pointer = context.build_alloca_at_entry(address_type, "address_pointer");
    let address_truncated =
        context
            .builder()
            .build_int_truncate(address, address_type, "address_truncated")?;
    let address_swapped = context.build_byte_swap(address_truncated.into())?;
    context.build_store(address_pointer, address_swapped)?;

    let extcodehash_pointer =
        context.build_alloca_at_entry(context.word_type(), "extcodehash_pointer");

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::CODE_HASH,
        &[
            address_pointer.to_int(context).into(),
            extcodehash_pointer.to_int(context).into(),
        ],
    );

    context.build_byte_swap(context.build_load(extcodehash_pointer, "extcodehash_value")?)
}
