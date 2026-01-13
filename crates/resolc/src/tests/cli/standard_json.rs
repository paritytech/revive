//! The tests for running resolc with standard JSON option.

use revive_solc_json_interface::{PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize};

use crate::cli_utils::{
    assert_command_success, assert_equal_exit_codes, assert_standard_json_errors_contain,
    execute_resolc_with_stdin_input, execute_solc_with_stdin_input, to_solc_standard_json_output,
    STANDARD_JSON_CONTRACTS_PATH, STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH,
    STANDARD_JSON_NO_EVM_CODEGEN_PATH,
};

const JSON_OPTION: &str = "--standard-json";

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
            arguments: vec![JSON_OPTION, "--heap-size", &default_heap_size_string],
            error_message:
                "Heap size must be specified in standard JSON input polkavm memory settings",
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
        TestCase {
            arguments: vec![JSON_OPTION, "--metadata-hash", "ipfs"],
            error_message:
                "`ipfs` metadata hash type is not supported. Please use `keccak256` instead",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--metadata-hash", "keccak256"],
            error_message: "Metadata hash mode must be specified in standard JSON input settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--asm"],
            error_message: "Cannot output assembly or binary outside of JSON in standard JSON mode",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--bin"],
            error_message: "Cannot output assembly or binary outside of JSON in standard JSON mode",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--libraries", "myLib=0x0000"],
            error_message: "Libraries must be passed via standard JSON input",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--evm-version", "osaka"],
            error_message: "EVM version must be passed via standard JSON input",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--output-dir", "tmp"],
            error_message: "Output directory cannot be used in standard JSON mode",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--overwrite"],
            error_message: "Overwriting flag cannot be used in standard JSON mode",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--disable-solc-optimizer"],
            error_message:
                "Disabling the solc optimizer must be specified in standard JSON input settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "-g"],
            error_message: "Debug info must be requested in standard JSON input polkavm settings",
        },
        TestCase {
            arguments: vec![JSON_OPTION, "--llvm-arg=-riscv"],
            error_message:
                "LLVM arguments must be configured in standard JSON input polkavm settings",
        },
    ];

    for case in cases {
        let result = execute_resolc_with_stdin_input(&case.arguments, STANDARD_JSON_CONTRACTS_PATH);
        let output = to_solc_standard_json_output(&result.stdout);
        assert_standard_json_errors_contain(&output, case.error_message);
    }
}
