//! The tests for running resolc when expecting usage output.

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc,
};

#[test]
fn shows_usage_with_help() {
    let arguments = &["--help"];
    let resolc_result = execute_resolc(arguments);
    assert_command_success(&resolc_result, "Providing the `--help` option");

    assert!(resolc_result.stdout.contains("Usage: resolc"));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_options() {
    let resolc_result = execute_resolc(&[]);
    assert_command_failure(&resolc_result, "Omitting options");

    assert!(resolc_result.stderr.contains("Usage: resolc"));

    let solc_result = execute_solc(&[]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}
