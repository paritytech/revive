//! The tests for running `resolc` with the heap and stack size options.

use crate::cli_utils::{
    absolute_path, assert_command_failure, execute_resolc, SOLIDITY_CONTRACT_PATH,
};

/// The panic message prefix `resolc` must never print.
const PANIC_MESSAGE: &str = "panicked";

/// Asserts that compiling the empty contract with `arguments` fails with `error_message`.
fn assert_compilation_error(arguments: &[&str], error_message: &str) {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let mut resolc_arguments = vec![path.as_str(), "--bin"];
    resolc_arguments.extend_from_slice(arguments);

    let result = execute_resolc(&resolc_arguments);
    assert_command_failure(&result, "Providing an out of range memory size");
    assert_eq!(result.code, revive_common::EXIT_CODE_FAILURE);
    assert!(
        !result.stderr.contains(PANIC_MESSAGE),
        "resolc should not panic: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains(error_message),
        "the error should contain `{error_message}`: {}",
        result.stderr
    );
}

/// A heap size outside the PVM memory map is rejected.
#[test]
fn heap_size_out_of_range() {
    assert_compilation_error(
        &["--heap-size", "4294967295"],
        "do not fit into the PVM memory map",
    );
}

/// A stack size outside the PVM memory map is rejected.
#[test]
fn stack_size_out_of_range() {
    assert_compilation_error(
        &["--stack-size", "4294967295"],
        "do not fit into the PVM memory map",
    );
}

/// A stack size the polkavm linker rejects is reported as an error.
#[test]
fn stack_size_linker_failure() {
    assert_compilation_error(
        &["--stack-size", "4294443009"],
        "polkavm linker failed: maximum memory size exceeded",
    );
}

/// A heap size the polkavm linker rejects is reported as an error.
#[test]
fn heap_size_linker_failure() {
    assert_compilation_error(
        &["--heap-size", "2147352561"],
        "polkavm linker failed: address overflow when casting",
    );
}
