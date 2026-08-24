use crate::cli_utils::{
    assert_command_success, execute_resolc, RESOLC_YUL_FLAG, SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH,
    YUL_CONTRACT_PATH,
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
/// instruction, and the polkavm linker decoded the encoding LLVM emitted for it. The fixture
/// is dense in 256-bit arithmetic, which is what carries the traffic: the masks and the
/// shifts that keep only part of a word are what the second width performs in a single
/// register instead of in a pair, and they survive every optimization level.
///
/// The negative half holds only while the width is behind the argument, and goes away with
/// it once the width ships unconditionally.
#[test]
fn llvm_argument_enables_the_128_bit_wide_instructions() {
    let output_with_argument = execute_resolc(&[
        SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH,
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

    let output_without_argument = execute_resolc(&[SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH, "--asm"]);
    assert_command_success(&output_without_argument, "Omitting the 128-bit width");
    assert!(
        !output_without_argument
            .stdout
            .contains(WIDE_INTEGER_128_MNEMONIC),
        "The 128-bit width should reach the assembly only on request."
    );
}
