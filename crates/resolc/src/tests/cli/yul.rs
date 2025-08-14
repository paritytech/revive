//! The tests for running resolc with yul option.

#![cfg(test)]

use revive_common;

use crate::tests::cli::utils;

const YUL_OPTION: &str = "--yul";
/// The `--yul` option was deprecated in Solidity 0.8.27 in favor of `--strict-assembly`.
/// See section `--strict-assembly vs. --yul` in https://soliditylang.org/blog/2024/09/04/solidity-0.8.27-release-announcement/
const SOLC_YUL_OPTION: &str = "--strict-assembly";
const CONTRACT_PATH: &str = "src/tests/cli/contracts/yul/contract.yul";

#[test]
fn can_run_with_valid_input_source() {
    const ARGUMENTS: &[&str] = &[CONTRACT_PATH, YUL_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    assert!(
        resolc_result.success,
        "Providing a valid input source should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        resolc_result.code,
        resolc_result.output
    );

    assert!(
        resolc_result.output.contains("Compiler run successful. No output requested"),
        "Expected the output to contain a success message."
    );

    const SOLC_ARGUMENTS: &[&str] = &[CONTRACT_PATH, SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(SOLC_ARGUMENTS);
    assert_eq!(
        solc_result.code,
        resolc_result.code,
        "Solc and resolc should have the same exit code."
    );
}

#[test]
fn fails_without_input_source() {
    const ARGUMENTS: &[&str] = &[YUL_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    assert!(
        !resolc_result.success,
        "Omitting an input source should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        resolc_result.code
    );

    assert!(
        resolc_result.output.contains("The input file is missing"),
        "Expected the output to contain a specific error message."
    );

    const SOLC_ARGUMENTS: &[&str] = &[SOLC_YUL_OPTION];
    let solc_result = utils::execute_solc(SOLC_ARGUMENTS);
    assert_eq!(
        solc_result.code,
        resolc_result.code,
        "Solc and resolc should have the same exit code."
    );
}
