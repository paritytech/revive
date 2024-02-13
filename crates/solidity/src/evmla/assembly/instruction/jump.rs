//!
//! Translates the jump operations.
//!

///
/// Translates the unconditional jump.
///
pub fn unconditional<D>(
    context: &mut era_compiler_llvm_context::EraVMContext<D>,
    destination: num::BigUint,
    stack_hash: md5::Digest,
) -> anyhow::Result<()>
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;
    let block_key = era_compiler_llvm_context::EraVMFunctionBlockKey::new(code_type, destination);

    let block = context
        .current_function()
        .borrow()
        .evmla()
        .find_block(&block_key, &stack_hash)?;
    context.build_unconditional_branch(block.inner());

    Ok(())
}

///
/// Translates the conditional jump.
///
pub fn conditional<D>(
    context: &mut era_compiler_llvm_context::EraVMContext<D>,
    destination: num::BigUint,
    stack_hash: md5::Digest,
    stack_height: usize,
) -> anyhow::Result<()>
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    let code_type = context
        .code_type()
        .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?;
    let block_key = era_compiler_llvm_context::EraVMFunctionBlockKey::new(code_type, destination);

    let condition_pointer = context.evmla().stack[stack_height]
        .to_llvm()
        .into_pointer_value();
    let condition = context.build_load(
        era_compiler_llvm_context::EraVMPointer::new_stack_field(context, condition_pointer),
        format!("conditional_{block_key}_condition").as_str(),
    );
    let condition = context.builder().build_int_compare(
        inkwell::IntPredicate::NE,
        condition.into_int_value(),
        context.field_const(0),
        format!("conditional_{block_key}_condition_compared").as_str(),
    )?;

    let then_block = context
        .current_function()
        .borrow()
        .evmla()
        .find_block(&block_key, &stack_hash)?;
    let join_block =
        context.append_basic_block(format!("conditional_{block_key}_join_block").as_str());

    context.build_conditional_branch(condition, then_block.inner(), join_block);

    context.set_basic_block(join_block);

    Ok(())
}
