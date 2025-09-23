//! The tests for running resolc with yul option.

#![cfg(test)]

use crate::tests::cli::utils;

pub const YUL_OPTION: &str = "--yul";
/// The `--yul` option was deprecated in Solidity 0.8.27 in favor of `--strict-assembly`.
/// See section `--strict-assembly vs. --yul` in https://soliditylang.org/blog/2024/09/04/solidity-0.8.27-release-announcement/
const SOLC_YUL_OPTION: &str = "--strict-assembly";

#[test]
fn runs_with_valid_input_file() {
    let arguments = &[utils::YUL_CONTRACT_PATH, YUL_OPTION];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_success(&resolc_result, "Providing a valid input file");

    assert!(resolc_result
        .stderr
        .contains("Compiler run successful. No output requested"));

    let solc_arguments = &[utils::YUL_CONTRACT_PATH, SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(solc_arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_input_file() {
    let arguments = &[YUL_OPTION];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Omitting an input file");

    assert!(resolc_result.stderr.contains("The input file is missing"));

    let solc_arguments = &[SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(solc_arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
