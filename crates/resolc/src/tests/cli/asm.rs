//! The tests for running resolc with asm option.

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc, RESOLC_NEWYORK_FLAG, RESOLC_YUL_FLAG, SOLIDITY_CONTRACT_PATH,
    YUL_ODD_OFFSET_WIDE_STORE_PATH,
};

const ASM_OPTION: &str = "--asm";

#[test]
fn runs_with_valid_input_file() {
    let arguments = &[SOLIDITY_CONTRACT_PATH, ASM_OPTION];
    let resolc_result = execute_resolc(arguments);
    assert_command_success(&resolc_result, "Providing a valid input file");

    for pattern in &["deploy", "call", "seal_return"] {
        assert!(
            resolc_result.stdout.contains(pattern),
            "Expected the output to contain `{pattern}`."
        );
    }

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

/// A wide store keeps an odd byte offset all the way into the linked blob.
///
/// The offset immediate of a wide memory access used to carry the access width in its low bit,
/// so every offset it could encode was even and an odd address had to be materialized into a
/// register first. The width now travels in `funct3` and the immediate is a plain byte-granular
/// `simm12`, which is what lets an odd offset stay folded into the instruction. The assembly is
/// disassembled from the linked blob, so this accounts for the whole stack at once: LLVM had to
/// select the store with the odd immediate, and the polkavm linker had to decode that encoding
/// back to the same offset. A toolchain on either of the old encodings emits an extra `addi`
/// and stores at offset 0, so this line is the one thing it cannot produce.
///
/// Only the newyork pipeline reaches an odd wide offset, because it is what makes unaligned
/// scalar memory legal; the stock Yul pipeline keeps its wide accesses aligned.
#[test]
fn an_odd_memory_offset_reaches_the_linked_blob() {
    // The 256-bit memory access, as the disassembler spells it.
    const WIDE_INTEGER_256_ACCESS: &str = "u256 [";
    // The offset the fixture's store folds to: the heap base is even, so address 1 is one past it.
    const ODD_MEMORY_OFFSET: &str = "+ 0x1]";

    let output = execute_resolc(&[
        RESOLC_YUL_FLAG,
        YUL_ODD_OFFSET_WIDE_STORE_PATH,
        RESOLC_NEWYORK_FLAG,
        ASM_OPTION,
    ]);
    assert_command_success(&output, "Storing at an odd memory offset");
    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.contains(WIDE_INTEGER_256_ACCESS) && line.contains(ODD_MEMORY_OFFSET)),
        "Expected a `{WIDE_INTEGER_256_ACCESS}` access at offset `{ODD_MEMORY_OFFSET}` in the \
         assembly. A toolchain that still encodes the memory width in the offset immediate \
         cannot hold an odd offset, and materializes the address into a register instead."
    );
}

#[test]
fn fails_without_input_file() {
    let arguments = &[ASM_OPTION];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Omitting an input file");

    let output = resolc_result.stderr.to_lowercase();
    assert!(output.contains("no input sources specified"));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}
