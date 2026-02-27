//! The tests for running resolc with asm option.

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc, SOLIDITY_CONTRACT_PATH,
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
