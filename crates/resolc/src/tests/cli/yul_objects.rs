//! The tests for Yul object shapes `resolc` does not support.
//!
//! `resolc` compiles every Yul object as a contract of its own, so two shapes `solc` accepts have
//! no representation: a sibling object that is not a contract, and the dotted notation for
//! addressing a nested object. Both used to abort the compiler; see
//! [paritytech/revive#351](https://github.com/paritytech/revive/issues/351).

use crate::cli_utils::{
    absolute_path, assert_command_failure, execute_resolc, CommandResult, RESOLC_YUL_FLAG,
    YUL_DOTTED_OBJECT_PATH, YUL_SIBLING_OBJECTS_PATH,
};

/// Asserts the command failed with a diagnostic rather than by aborting.
#[track_caller]
fn assert_reported_not_aborted(result: &CommandResult, description: &str) {
    assert_command_failure(result, description);

    let output = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !output.contains("panicked") && !output.contains("ICE:"),
        "{description} should be reported, not abort the compiler, got: {output}"
    );
}

#[test]
fn reports_a_sibling_object_that_is_not_a_contract() {
    let path = absolute_path(YUL_SIBLING_OBJECTS_PATH);
    let result = execute_resolc(&[&path, RESOLC_YUL_FLAG, "--bin"]);

    assert_reported_not_aborted(&result, "A sibling object that is not a contract");
    assert!(
        result.stderr.contains("Error"),
        "the object should be named in the diagnostic, got: {}",
        result.stderr
    );
}

#[test]
fn reports_the_dotted_object_notation() {
    let path = absolute_path(YUL_DOTTED_OBJECT_PATH);
    let result = execute_resolc(&[&path, RESOLC_YUL_FLAG, "--bin"]);

    assert_reported_not_aborted(&result, "The dotted notation for a nested object");
    assert!(
        result.stderr.contains("dotted"),
        "the diagnostic should explain the unsupported notation, got: {}",
        result.stderr
    );
}
