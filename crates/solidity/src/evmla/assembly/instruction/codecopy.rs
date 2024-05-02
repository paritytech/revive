//! Translates the CODECOPY use cases.

/// Translates the contract hash copying.
pub fn contract_hash<'ctx, D>(
    context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    let offset = context.builder().build_int_add(
        offset,
        context.field_const(
            (revive_common::BYTE_LENGTH_X32 + revive_common::BYTE_LENGTH_FIELD) as u64,
        ),
        "datacopy_contract_hash_offset",
    )?;

    revive_llvm_context::polkavm_evm_memory::store(context, offset, value)?;

    Ok(())
}

/// Translates the library marker copying.
pub fn library_marker<D>(
    context: &mut revive_llvm_context::PolkaVMContext<D>,
    offset: u64,
    value: u64,
) -> anyhow::Result<()>
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    revive_llvm_context::polkavm_evm_memory::store_byte(
        context,
        context.field_const(offset),
        context.field_const(value),
    )?;

    Ok(())
}

/// Translates the static data copying.
pub fn static_data<'ctx, D>(
    context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    destination: inkwell::values::IntValue<'ctx>,
    source: &str,
) -> anyhow::Result<()>
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    let mut offset = 0;
    for (index, chunk) in source
        .chars()
        .collect::<Vec<char>>()
        .chunks(revive_common::BYTE_LENGTH_FIELD * 2)
        .enumerate()
    {
        let mut value_string = chunk.iter().collect::<String>();
        value_string.push_str(
            "0".repeat((revive_common::BYTE_LENGTH_FIELD * 2) - chunk.len())
                .as_str(),
        );

        let datacopy_destination = context.builder().build_int_add(
            destination,
            context.field_const(offset as u64),
            format!("datacopy_destination_index_{index}").as_str(),
        )?;
        let datacopy_value = context.field_const_str_hex(value_string.as_str());
        revive_llvm_context::polkavm_evm_memory::store(
            context,
            datacopy_destination,
            datacopy_value,
        )?;
        offset += chunk.len() / 2;
    }

    Ok(())
}
