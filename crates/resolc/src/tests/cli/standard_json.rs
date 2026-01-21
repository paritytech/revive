//! The tests for running resolc with standard JSON option.

use revive_solc_json_interface::{
    PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize, SolcStandardJsonOutput,
};

use crate::cli_utils::{
    assert_command_success, assert_equal_exit_codes, execute_resolc_with_stdin_input,
    execute_solc_with_stdin_input, STANDARD_JSON_CONTRACTS_PATH,
    STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH, STANDARD_JSON_NO_EVM_CODEGEN_PATH,
    STANDARD_JSON_NO_PVM_CODEGEN_ALL_WILDCARD_PATH, STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH,
};

const JSON_OPTION: &str = "--standard-json";

/// Asserts that the standard JSON output has at least one error with the given `error_message`.
fn assert_standard_json_errors_contain(output: &SolcStandardJsonOutput, error_message: &str) {
    assert!(
        output
            .errors
            .iter()
            .any(|error| error.is_error() && error.message.contains(error_message)),
        "the standard JSON output should contain the error message `{error_message}`"
    );
}

/// Asserts that the standard JSON output has no errors with severity `error`.
fn assert_no_errors(output: &SolcStandardJsonOutput) {
    assert!(!output.errors.iter().any(|error| error.is_error()));
}

/// Converts valid JSON text to `SolcStandardJsonOutput`.
fn to_solc_standard_json_output(json_text: &str) -> SolcStandardJsonOutput {
    serde_json::from_str(json_text).unwrap()
}

#[test]
fn runs_with_valid_input_file() {
    let arguments = &[JSON_OPTION];
    let resolc_result = execute_resolc_with_stdin_input(arguments, STANDARD_JSON_CONTRACTS_PATH);
    assert_command_success(&resolc_result, "Providing a valid input file to stdin");

    let resolc_output = to_solc_standard_json_output(&resolc_result.stdout);
    assert_no_errors(&resolc_output);

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
    assert_no_errors(&output);
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
    assert_no_errors(&output);
}

/// Asserts that no PVM codegen output has been generated.
///
/// This assert is specific to the files `STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH`
/// and `STANDARD_JSON_NO_PVM_CODEGEN_ALL_WILDCARD_PATH`.
fn assert_no_pvm_codegen(output: &SolcStandardJsonOutput) {
    // Assert that no extra file-level output required for codegen exist.
    for (path, source) in output.sources.iter() {
        assert!(
            source.ast.is_none(),
            "the AST for the file `{path}` should not be generated",
        );
    }

    assert!(
        !output.contracts.is_empty(),
        "contracts should be generated"
    );

    for (_, contracts) in output.contracts.iter() {
        for (name, contract) in contracts.iter() {
            // Assert that no codegen output exist.
            let evm = contract.evm.as_ref().unwrap();
            assert!(
                evm.bytecode.is_none(),
                "the bytecode for the contract `{name}` should not be generated",
            );
            assert!(
                evm.deployed_bytecode.is_none(),
                "the deployed bytecode for the contract `{name}` should not be generated",
            );
            assert!(
                evm.assembly_text.is_none(),
                "the assembly for the contract `{name}` should not be generated",
            );

            // Assert that no extra contract-level output required for codegen exist.
            assert!(
                contract.ir_optimized.is_empty(),
                "the Yul for the contract `{name}` should not be generated",
            );

            // Assert that the requested output exists.
            assert!(
                !contract.abi.is_null(),
                "the abi for the contract `{name}` should be generated",
            );
            assert!(
                !contract.metadata.is_null(),
                "the metadata for the contract `{name}` should be generated",
            );
        }
    }
}

#[test]
fn no_pvm_codegen_requested_per_file() {
    let result =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH);
    assert_command_success(&result, "the no PVM codegen std JSON input fixture");

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);
    assert_no_pvm_codegen(&output);
}

#[test]
fn no_pvm_codegen_requested_for_all_files() {
    let result = execute_resolc_with_stdin_input(
        &[JSON_OPTION],
        STANDARD_JSON_NO_PVM_CODEGEN_ALL_WILDCARD_PATH,
    );
    assert_command_success(&result, "the no PVM codegen std JSON input fixture");

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);
    assert_no_pvm_codegen(&output);
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
