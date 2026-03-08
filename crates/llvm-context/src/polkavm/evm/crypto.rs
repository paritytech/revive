//! Translates the cryptographic operations.

use crate::polkavm::context::Context;

/// Translates the `sha3` instruction.
pub fn sha3<'ctx>(
    context: &mut Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    let offset_casted = context.safe_truncate_int_to_xlen(offset)?;
    let length_casted = context.safe_truncate_int_to_xlen(length)?;
    let input_pointer = context.build_heap_gep(offset_casted, length_casted)?;
    let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");

    context.build_runtime_call(
        revive_runtime_api::polkavm_imports::HASH_KECCAK_256,
        &[
            input_pointer.to_int(context).into(),
            length_casted.into(),
            output_pointer.to_int(context).into(),
        ],
    );

    context.build_byte_swap(context.build_load(output_pointer, "sha3_output")?)
}

/// Translates keccak256 of one 256-bit word via a deduplicated helper function.
/// Equivalent to: mstore(0, word0); sha3(0, 32)
/// but emitted as a single function call to reduce code size.
///
/// Falls back to inline code if the helper function is not declared
/// (e.g. when there are too few call sites to justify the function body cost).
pub fn sha3_one_word<'ctx>(
    context: &mut Context<'ctx>,
    word0: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    use crate::polkavm::context::pointer::heap::Keccak256OneWord;
    if let Some(function) = context.get_function(Keccak256OneWord::NAME, false) {
        let result = context
            .build_call(
                function.borrow().declaration(),
                &[word0.into()],
                "keccak256_single_result",
            )
            .expect("should always return a value");
        Ok(result)
    } else {
        // Inline fallback: mstore(0, word0); sha3(0, 32)
        let offset = context.word_const(0);
        let length = context.word_const(32);
        crate::polkavm::evm::memory::store(context, offset, word0)?;
        sha3(context, offset, length)
    }
}

/// Translates keccak256 of two 256-bit words via a deduplicated helper function.
/// Equivalent to: mstore(0, word0); mstore(32, word1); sha3(0, 64)
/// but emitted as a single function call to reduce code size.
pub fn sha3_two_words<'ctx>(
    context: &mut Context<'ctx>,
    word0: inkwell::values::IntValue<'ctx>,
    word1: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
    use crate::polkavm::context::pointer::heap::Keccak256TwoWords;
    let function = context
        .get_function(Keccak256TwoWords::NAME, false)
        .expect("__revive_keccak256_two_words should be declared");
    let result = context
        .build_call(
            function.borrow().declaration(),
            &[word0.into(), word1.into()],
            "keccak256_pair_result",
        )
        .expect("should always return a value");
    Ok(result)
}
