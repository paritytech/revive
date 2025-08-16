//! The tests for running resolc when expecting usage output.

#![cfg(test)]

use crate::tests::cli::utils;

#[test]
#[ignore = "Fix: 'resolc --help' should exit with success exit code"]
fn shows_usage_with_help() {
    const ARGUMENTS: &[&str] = &["--help"];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    utils::assert_command_success(&resolc_result, "Providing the `--help` option");

    assert!(
        resolc_result.output.contains("Usage: resolc"),
        "Expected the output to contain usage information."
    );

    let solc_result = utils::execute_solc(ARGUMENTS);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

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
