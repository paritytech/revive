//! Some LLVM IR generator utilies.

use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// Clamps `value` to `max_value`, if `value` is bigger than `max_value`.
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

    let pointer = context.build_alloca(context.word_type(), format!("{name}_pointer").as_str());
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

/// Computes the `keccak256` hash for `preimage`.
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
