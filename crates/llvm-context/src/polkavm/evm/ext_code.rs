//! Translates the external code operations.

use inkwell::values::BasicValue;

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
    let address_pointer = match address {
        Some(address) => {
            let address_pointer = context.build_alloca(context.word_type(), "value");
            context.build_store(address_pointer, address)?;
            address_pointer
        }
        None => context.sentinel_pointer(),
    };

    let address_pointer_casted = context.builder().build_ptr_to_int(
        address_pointer.value,
        context.xlen_type(),
        "address_pointer",
    )?;
    // TODO: uncomment when code_size is implemented on pallet-revive
    // let value = context
    //     .build_runtime_call(
    //         revive_runtime_api::polkavm_imports::CODE_SIZE,
    //         &[address_pointer_casted.into()],
    //     )
    //     .unwrap_or_else(|| {
    //         panic!(
    //             "{} should return a value",
    //             revive_runtime_api::polkavm_imports::CODE_SIZE
    //         )
    //     })
    //     .into_int_value();
    let value = context.word_type().const_int(1000, false); // dummy size

    Ok(context
        .builder()
        .build_int_z_extend(value, context.word_type(), "extcodesize")?
        .as_basic_value_enum())
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
