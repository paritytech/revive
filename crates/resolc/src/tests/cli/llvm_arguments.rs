use crate::cli_utils::{
    assert_command_success, execute_resolc, RESOLC_NEWYORK_FLAG, RESOLC_YUL_FLAG,
    SOLIDITY_RECURSIVE_UINT128_ARITHMETIC_PATH, YUL_CONTRACT_PATH,
};

/// Makes `i128` a legal type held in a single vector register, next to the `i256` that the
/// wide integer extension holds in a register pair. Hidden in LLVM and off by default,
/// until the second width ships unconditionally.
const WIDE_INTEGER_128_ARGUMENT: &str = "--llvm-arg=-riscv-revive-i128";

/// The width the disassembler prints into the mnemonic of a 128-bit wide instruction.
const WIDE_INTEGER_128_MNEMONIC: &str = "w128";

#[test]
fn llvm_arguments_work_with_yul_input() {
    let output_with_argument = execute_resolc(&[
        RESOLC_YUL_FLAG,
        YUL_CONTRACT_PATH,
        "--llvm-arg=-riscv-soften-spills'",
        "--bin",
    ]);
    assert_command_success(&output_with_argument, "Providing LLVM arguments");
    assert!(output_with_argument.success);
}

/// The 128-bit width of the wide integer extension selects only when asked for.
///
/// The assembly is disassembled from the linked blob, so a 128-bit mnemonic in it accounts
/// for the whole stack at once: the argument made the type legal, LLVM selected the
/// instruction, and the polkavm linker decoded the encoding LLVM emitted for it.
///
/// The compile goes through newyork because its type narrowing is what manufactures a width
/// between 64 and 256 bits: it narrows the fixture's helper parameters to 128 bits, so the call
/// boundary and the arithmetic feeding it carry that width, which the second width holds in a
/// single register instead of in a pair. The stock Yul path has no such pass; the 128-bit
/// instructions it used to emit were the byte-wise recombination of an unaligned heap word,
/// which a single wide load now performs.
///
/// The negative half holds only while the width is behind the argument, and goes away with
/// it once the width ships unconditionally.
#[test]
fn llvm_argument_enables_the_128_bit_wide_instructions() {
    let output_with_argument = execute_resolc(&[
        SOLIDITY_RECURSIVE_UINT128_ARITHMETIC_PATH,
        RESOLC_NEWYORK_FLAG,
        "--asm",
        WIDE_INTEGER_128_ARGUMENT,
    ]);
    assert_command_success(&output_with_argument, "Requesting the 128-bit width");
    assert!(
        output_with_argument
            .stdout
            .contains(WIDE_INTEGER_128_MNEMONIC),
        "Expected `{WIDE_INTEGER_128_MNEMONIC}` instructions in the assembly. \
         A toolchain older than the pinned LLVM and polkavm revisions emits none."
    );

    let output_without_argument = execute_resolc(&[
        SOLIDITY_RECURSIVE_UINT128_ARITHMETIC_PATH,
        RESOLC_NEWYORK_FLAG,
        "--asm",
    ]);
    assert_command_success(&output_without_argument, "Omitting the 128-bit width");
    assert!(
        !output_without_argument
            .stdout
            .contains(WIDE_INTEGER_128_MNEMONIC),
        "The 128-bit width should reach the assembly only on request."
    );
}
