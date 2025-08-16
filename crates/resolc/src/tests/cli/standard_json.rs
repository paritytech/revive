//! The tests for running resolc with standard JSON option.

#![cfg(test)]

use revive_common;

use crate::tests::cli::utils;

const JSON_OPTION: &str = "--standard-json";

#[test]
fn runs_with_valid_input_file() {
    const ARGUMENTS: &[&str] = &[JSON_OPTION];
    let resolc_result =
        utils::execute_resolc_with_stdin_input(ARGUMENTS, utils::STANDARD_JSON_CONTRACTS_PATH);
    assert!(
        resolc_result.success,
        "Providing a valid input file to stdin should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        resolc_result.code,
        resolc_result.output
    );

    assert!(
        resolc_result.output.contains("contracts"),
        "Expected the output to contain a `contracts` field."
    );

    let solc_result =
        utils::execute_solc_with_stdin_input(ARGUMENTS, utils::STANDARD_JSON_CONTRACTS_PATH);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
