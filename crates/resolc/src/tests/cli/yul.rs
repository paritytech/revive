//! The tests for running resolc with yul option.

#![cfg(test)]

use revive_common;

use crate::tests::cli::utils;

pub const YUL_OPTION: &str = "--yul";
// The `--yul` option was deprecated in Solidity 0.8.27 in favor of `--strict-assembly`.
// See section `--strict-assembly vs. --yul` in https://soliditylang.org/blog/2024/09/04/solidity-0.8.27-release-announcement/
const SOLC_YUL_OPTION: &str = "--strict-assembly";

#[test]
fn runs_with_valid_input_file() {
    const ARGUMENTS: &[&str] = &[utils::YUL_CONTRACT_PATH, YUL_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    assert!(
        resolc_result.success,
        "Providing a valid input file should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        resolc_result.code,
        resolc_result.output
    );

    assert!(
        resolc_result
            .output
            .contains("Compiler run successful. No output requested"),
        "Expected the output to contain a success message."
    );

    const SOLC_ARGUMENTS: &[&str] = &[utils::YUL_CONTRACT_PATH, SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(SOLC_ARGUMENTS);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_input_file() {
    const ARGUMENTS: &[&str] = &[YUL_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    utils::assert_command_failure(&resolc_result, "Omitting an input file");

    assert!(
        resolc_result.output.contains("The input file is missing"),
        "Expected the output to contain a specific error message."
    );

    const SOLC_ARGUMENTS: &[&str] = &[SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(SOLC_ARGUMENTS);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
