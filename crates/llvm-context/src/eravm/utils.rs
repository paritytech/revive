//!
//! Some LLVM IR generator utilies.
//!

use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::function::llvm_runtime::LLVMRuntime;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Clamps `value` to `max_value`, if `value` is bigger than `max_value`.
///
pub fn clamp<'ctx, D>(
    context: &mut Context<'ctx, D>,
    value: inkwell::values::IntValue<'ctx>,
    max_value: inkwell::values::IntValue<'ctx>,
    name: &str,
) -> anyhow::Result<inkwell::values::IntValue<'ctx>>
where
    D: Dependency + Clone,
{
    let in_bounds_block = context.append_basic_block(format!("{name}_is_bounds_block").as_str());
    let join_block = context.append_basic_block(format!("{name}_join_block").as_str());

    let pointer = context.build_alloca(context.field_type(), format!("{name}_pointer").as_str());
    context.build_store(pointer, max_value)?;

    let is_in_bounds = context.builder().build_int_compare(
        inkwell::IntPredicate::ULE,
        value,
        max_value,
        format!("{name}_is_in_bounds").as_str(),
    )?;
    context.build_conditional_branch(is_in_bounds, in_bounds_block, join_block)?;

    context.set_basic_block(in_bounds_block);
    context.build_store(pointer, value)?;
    context.build_unconditional_branch(join_block);

    context.set_basic_block(join_block);
    let result = context.build_load(pointer, name)?;
    Ok(result.into_int_value())
}

///
/// Generates an exception.
///
pub fn throw<D>(context: &Context<D>)
where
    D: Dependency + Clone,
{
    context.build_call(
        context.llvm_runtime().cxa_throw,
        &[context
            .byte_type()
            .ptr_type(AddressSpace::Stack.into())
            .get_undef()
            .as_basic_value_enum(); 3],
        LLVMRuntime::FUNCTION_CXA_THROW,
    );
    context.build_unreachable();
}

///
/// Returns the full list of arguments for an external call.
///
/// Performs the extra ABI data padding and adds the mimic call extra argument.
///
pub fn external_call_arguments<'ctx, D>(
    _context: &Context<'ctx, D>,
    abi_data: inkwell::values::BasicValueEnum<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    _extra_abi_data: Vec<inkwell::values::IntValue<'ctx>>,
    mimic: Option<inkwell::values::IntValue<'ctx>>,
) -> Vec<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let mut result = Vec::with_capacity(
        crate::eravm::context::function::runtime::entry::Entry::MANDATORY_ARGUMENTS_COUNT
            + crate::eravm::EXTRA_ABI_DATA_SIZE
            + usize::from(mimic.is_some()),
    );
    result.push(abi_data);
    result.push(address.as_basic_value_enum());
    //result.extend(
    //    pad_extra_abi_data(context, extra_abi_data)
    //        .into_iter()
    //        .map(|value| value.as_basic_value_enum()),
    //);
    if let Some(mimic) = mimic {
        result.push(mimic.as_basic_value_enum());
    }
    result
}

///
/// Generates an ABI data for an external call.
///
/// If `gas` is `None`, it is fetched from the contract context.
///
pub fn abi_data<'ctx, D>(
    context: &mut Context<'ctx, D>,
    input_offset: inkwell::values::IntValue<'ctx>,
    input_length: inkwell::values::IntValue<'ctx>,
    gas: Option<inkwell::values::IntValue<'ctx>>,
    address_space: AddressSpace,
    is_system_call: bool,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let input_offset = crate::eravm::utils::clamp(
        context,
        input_offset,
        context.field_const(u32::MAX as u64),
        "abi_data_input_offset",
    )?;
    let input_length = crate::eravm::utils::clamp(
        context,
        input_length,
        context.field_const(u32::MAX as u64),
        "abi_data_input_length",
    )?;

    let gas = match gas {
        Some(gas) => gas,
        None => crate::eravm::evm::ether_gas::gas(context)?.into_int_value(),
    };
    let gas = crate::eravm::utils::clamp(
        context,
        gas,
        context.field_const(u32::MAX as u64),
        "abi_data_gas",
    )?;

    let input_offset_shifted = context.builder().build_left_shift(
        input_offset,
        context.field_const((revive_common::BIT_LENGTH_X32 * 2) as u64),
        "abi_data_input_offset_shifted",
    )?;
    let input_length_shifted = context.builder().build_left_shift(
        input_length,
        context.field_const((revive_common::BIT_LENGTH_X32 * 3) as u64),
        "abi_data_input_length_shifted",
    )?;
    let gas_shifted = context.builder().build_left_shift(
        gas,
        context.field_const((revive_common::BIT_LENGTH_X32 * 6) as u64),
        "abi_data_gas_shifted",
    )?;

    let mut abi_data = context.builder().build_int_add(
        input_offset_shifted,
        input_length_shifted,
        "abi_data_offset_and_length",
    )?;
    abi_data = context
        .builder()
        .build_int_add(abi_data, gas_shifted, "abi_data_add_gas")?;
    if let AddressSpace::HeapAuxiliary = address_space {
        let auxiliary_heap_marker_shifted = context.builder().build_left_shift(
            context.field_const(zkevm_opcode_defs::FarCallForwardPageType::UseAuxHeap as u64),
            context.field_const((revive_common::BIT_LENGTH_X32 * 7) as u64),
            "abi_data_auxiliary_heap_marker_shifted",
        )?;
        abi_data = context.builder().build_int_add(
            abi_data,
            auxiliary_heap_marker_shifted,
            "abi_data_add_heap_auxiliary_marker",
        )?;
    }
    if is_system_call {
        let auxiliary_heap_marker_shifted = context.builder().build_left_shift(
            context.field_const(zkevm_opcode_defs::FarCallForwardPageType::UseAuxHeap as u64),
            context.field_const(
                ((revive_common::BIT_LENGTH_X32 * 7) + (revive_common::BIT_LENGTH_BYTE * 3)) as u64,
            ),
            "abi_data_system_call_marker_shifted",
        )?;
        abi_data = context.builder().build_int_add(
            abi_data,
            auxiliary_heap_marker_shifted,
            "abi_data_add_system_call_marker",
        )?;
    }

    Ok(abi_data.as_basic_value_enum())
}

///
/// Pads the extra ABI data with `i256::undef`, so it always consists of 10 values.
///
pub fn pad_extra_abi_data<'ctx, D>(
    context: &Context<'ctx, D>,
    initial_data: Vec<inkwell::values::IntValue<'ctx>>,
) -> [inkwell::values::IntValue<'ctx>; crate::eravm::EXTRA_ABI_DATA_SIZE]
where
    D: Dependency + Clone,
{
    let mut padded_data = initial_data;
    padded_data.extend(vec![
        context.field_undef();
        crate::eravm::EXTRA_ABI_DATA_SIZE - padded_data.len()
    ]);
    padded_data.try_into().expect("Always valid")
}

///
/// Computes the `keccak256` hash for `preimage`.
///
pub fn keccak256(preimage: &[u8]) -> String {
    use sha3::Digest;

    let hash_bytes = sha3::Keccak256::digest(preimage);
    hash_bytes
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<String>>()
        .join("")
}

#[cfg(test)]
mod tests {
    #[test]
    fn keccak256() {
        assert_eq!(
            super::keccak256("zksync".as_bytes()),
            "0238fb1ab06c28c32885f9a4842207ac480c2467df26b6c58e201679628c5a5b"
        );
    }
}
