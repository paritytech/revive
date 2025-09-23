//! The tests for running resolc with yul option.

#![cfg(test)]

use crate::tests::cli::utils::{
    assert_command_success, assert_equal_exit_codes, execute_resolc, execute_solc, RESOLC_YUL_FLAG,
    SOLC_YUL_FLAG, YUL_CONTRACT_PATH,
};

#[test]
fn runs_with_valid_input_file() {
    let resolc_result = execute_resolc(&[YUL_CONTRACT_PATH, RESOLC_YUL_FLAG]);
    assert_command_success(&resolc_result, "Providing a valid input file");

    assert!(resolc_result
        .stderr
        .contains("Compiler run successful. No output requested"));

    let solc_result = execute_solc(&[YUL_CONTRACT_PATH, SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

/// While the `solc` Solidity mode requires output selection,
/// the strict-assembly mode does not.
///
/// `resolc` exhibits consistent behavior for both modes.
#[test]
fn runs_without_input_file() {
    let resolc_result = execute_resolc(&[RESOLC_YUL_FLAG]);
    assert_command_success(&resolc_result, "Omitting an input file");
    assert!(resolc_result
        .stderr
        .contains("Compiler run successful. No output requested"));
}
