//!
//! Translates the CODECOPY use cases.
//!

///
/// Translates the contract hash copying.
///
pub fn contract_hash<'ctx, D>(
    context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()>
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    let offset = context.builder().build_int_add(
        offset,
        context.field_const(
            (era_compiler_common::BYTE_LENGTH_X32 + era_compiler_common::BYTE_LENGTH_FIELD) as u64,
        ),
        "datacopy_contract_hash_offset",
    )?;

    era_compiler_llvm_context::eravm_evm_memory::store(context, offset, value)?;

    Ok(())
}

///
/// Translates the library marker copying.
///
pub fn library_marker<D>(
    context: &mut era_compiler_llvm_context::EraVMContext<D>,
    offset: u64,
    value: u64,
) -> anyhow::Result<()>
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    era_compiler_llvm_context::eravm_evm_memory::store_byte(
        context,
        context.field_const(offset),
        context.field_const(value),
    )?;

    Ok(())
}

///
/// Translates the static data copying.
///
pub fn static_data<'ctx, D>(
    context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    destination: inkwell::values::IntValue<'ctx>,
    source: &str,
) -> anyhow::Result<()>
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    let mut offset = 0;
    for (index, chunk) in source
        .chars()
        .collect::<Vec<char>>()
        .chunks(era_compiler_common::BYTE_LENGTH_FIELD * 2)
        .enumerate()
    {
        let mut value_string = chunk.iter().collect::<String>();
        value_string.push_str(
            "0".repeat((era_compiler_common::BYTE_LENGTH_FIELD * 2) - chunk.len())
                .as_str(),
        );

        let datacopy_destination = context.builder().build_int_add(
            destination,
            context.field_const(offset as u64),
            format!("datacopy_destination_index_{index}").as_str(),
        )?;
        let datacopy_value = context.field_const_str_hex(value_string.as_str());
        era_compiler_llvm_context::eravm_evm_memory::store(
            context,
            datacopy_destination,
            datacopy_value,
        )?;
        offset += chunk.len() / 2;
    }

    Ok(())
}
