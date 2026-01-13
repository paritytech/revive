//! The tests for running resolc with standard JSON option.

use revive_solc_json_interface::{
    PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize, SolcStandardJsonOutput,
};

use crate::cli_utils::{
    assert_command_success, assert_equal_exit_codes, execute_resolc_with_stdin_input,
    execute_solc_with_stdin_input, STANDARD_JSON_CONTRACTS_PATH,
    STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH, STANDARD_JSON_NO_EVM_CODEGEN_PATH,
};

const JSON_OPTION: &str = "--standard-json";

/// Converts valid JSON text to `SolcStandardJsonOutput`.
fn to_solc_standard_json_output(json_text: &str) -> SolcStandardJsonOutput {
    serde_json::from_str(json_text).unwrap()
}

fn assert_standard_json_errors_contain(output: &SolcStandardJsonOutput, message: &str) {
    assert!(
        output
            .errors
            .iter()
            .any(|error| error.message.contains(message)),
        "the standard JSON output should contain the error message `{message}`"
    );
}

#[test]
fn runs_with_valid_input_file() {
    let arguments = &[JSON_OPTION];
    let resolc_result = execute_resolc_with_stdin_input(arguments, STANDARD_JSON_CONTRACTS_PATH);
    assert_command_success(&resolc_result, "Providing a valid input file to stdin");

    assert!(
        resolc_result.stdout.contains("contracts"),
        "Expected the output to contain a `contracts` field."
    );

    let solc_result = execute_solc_with_stdin_input(arguments, STANDARD_JSON_CONTRACTS_PATH);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn no_evm_codegen_requested() {
    let result = execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NO_EVM_CODEGEN_PATH);
    assert_command_success(&result, "EVM codegen std json input fixture should build");

    let output = to_solc_standard_json_output(&result.stdout);
    assert!(!output.errors.iter().any(|msg| msg.severity == "error"))
}

/// A variant with more complex contracts.
///
/// The fixture is from a real project known to trigger the "Stack too deep" error.
///
/// The test ensures we set the right flags when requesting the Yul IR from solc:
/// no EVM codegen should be involved.
#[test]
fn no_evm_codegen_requested_complex() {
    let result =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH);
    assert_command_success(
        &result,
        "the EVM codegen std json complext input fixture should build fine",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert!(!output.errors.iter().any(|msg| msg.severity == "error"))
}

#[test]
fn invalid_extra_arguments() {
    struct TestCase<'a> {
        arguments: Vec<&'a str>,
        error_message: &'static str,
    }

    let default_heap_size_string = PolkaVMDefaultHeapMemorySize.to_string();
    let default_stack_size_string = PolkaVMDefaultStackMemorySize.to_string();

    let cases = vec![
        TestCase {
            arguments: vec![JSON_OPTION, "--heap-size", "50000"],
            error_message:
                "Heap size must be specified in standard JSON input polkavm memory settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--heap-size", &default_heap_size_string],
            error_message:
                "Heap size must be specified in standard JSON input polkavm memory settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--stack-size", "50000"],
            error_message:
                "Stack size must be specified in standard JSON input polkavm memory settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--stack-size", &default_stack_size_string],
            error_message:
                "Stack size must be specified in standard JSON input polkavm memory settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--optimization", "z"],
            error_message: "LLVM optimizations must be specified in standard JSON input settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "-Oz"],
            error_message: "LLVM optimizations must be specified in standard JSON input settings",
        },
    ];

    for case in cases {
        let result = execute_resolc_with_stdin_input(&case.arguments, STANDARD_JSON_CONTRACTS_PATH);
        let output = to_solc_standard_json_output(&result.stdout);
        assert_standard_json_errors_contain(&output, case.error_message);
    }
}
