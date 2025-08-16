//! The tests for running resolc with asm option.

#![cfg(test)]

use crate::tests::cli::utils;

const ASM_OPTION: &str = "--asm";

#[test]
fn runs_with_valid_input_file() {
    const ARGUMENTS: &[&str] = &[utils::SOLIDITY_CONTRACT_PATH, ASM_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    utils::assert_command_success(&resolc_result, "Providing a valid input file");

    for pattern in &["deploy", "call", "seal_return"] {
        assert!(
            resolc_result.output.contains(pattern),
            "Expected the output to contain `{pattern}`."
        );
    }

    let solc_result = utils::execute_solc(ARGUMENTS);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_input_file() {
    const ARGUMENTS: &[&str] = &[ASM_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    utils::assert_command_failure(&resolc_result, "Omitting an input file");

    let output = resolc_result.output.to_lowercase();
    assert!(
        output.contains("no input sources specified") || output.contains("compilation aborted"),
        "Expected the output to contain a specific error message."
    );

    let solc_result = utils::execute_solc(ARGUMENTS);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
