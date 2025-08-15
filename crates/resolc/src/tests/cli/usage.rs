//! The tests for running resolc when expecting usage output.

#![cfg(test)]

use crate::tests::cli::utils;

#[test]
fn fails_without_options() {
    let resolc_result = utils::execute_resolc(&[]);
    utils::assert_command_failure(&resolc_result, "Omitting options");

    assert!(
        resolc_result.output.contains("Usage: resolc"),
        "Expected the output to contain usage information."
    );

    let solc_result = utils::execute_solc(&[]);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
