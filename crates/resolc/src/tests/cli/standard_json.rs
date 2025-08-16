//! The tests for running resolc with standard JSON option.

#![cfg(test)]

use crate::tests::cli::utils;

const JSON_OPTION: &str = "--standard-json";

#[test]
fn runs_with_valid_input_file() {
    const ARGUMENTS: &[&str] = &[JSON_OPTION];
    let resolc_result =
        utils::execute_resolc_with_stdin_input(ARGUMENTS, utils::STANDARD_JSON_CONTRACTS_PATH);
    utils::assert_command_success(&resolc_result, "Providing a valid input file to stdin");

    assert!(
        resolc_result.output.contains("contracts"),
        "Expected the output to contain a `contracts` field."
    );

    let solc_result =
        utils::execute_solc_with_stdin_input(ARGUMENTS, utils::STANDARD_JSON_CONTRACTS_PATH);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
