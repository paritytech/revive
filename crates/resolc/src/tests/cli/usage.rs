//! The tests for running resolc when expecting usage output.

use crate::tests::cli::utils;

#[test]
fn shows_usage_with_help() {
    let arguments = &["--help"];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_success(&resolc_result, "Providing the `--help` option");

    assert!(resolc_result.stdout.contains("Usage: resolc"));

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_options() {
    let resolc_result = utils::execute_resolc(&[]);
    utils::assert_command_failure(&resolc_result, "Omitting options");

    assert!(resolc_result.stderr.contains("Usage: resolc"));

    let solc_result = utils::execute_solc(&[]);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}
