//! The tests for running resolc with standard JSON option.

use revive_solc_json_interface::{
    PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize,
    SolcStandardJsonInputSettingsSelectionFileFlag, SolcStandardJsonOutput,
};

use crate::cli_utils::{
    assert_command_success, assert_equal_exit_codes, execute_resolc_with_stdin_input,
    execute_solc_with_stdin_input, STANDARD_JSON_CONTRACTS_PATH,
    STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH, STANDARD_JSON_NO_EVM_CODEGEN_PATH,
    STANDARD_JSON_NO_PVM_CODEGEN_ALL_WILDCARD_PATH, STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH,
};

const JSON_OPTION: &str = "--standard-json";

/// A subset of contracts expected to exist in the output.
struct ExpectedOutput {
    contracts: Vec<ExpectedContract>,
}

/// A contract with specific fields, such as `metadata`, `evm.bytecode`,
/// `irOptimized`, etc., expected to exist in the output.
struct ExpectedContract {
    path: &'static str,
    name: &'static str,
    fields: Vec<&'static str>,
}

/// Asserts that the `expected` contracts match the ones in the `actual` output.
fn assert_output_matches(actual: &SolcStandardJsonOutput, expected: &ExpectedOutput) {
    assert!(
        !expected.contracts.is_empty(),
        "select which contracts should be validated"
    );
    assert!(
        !actual.contracts.is_empty(),
        "contracts should be generated"
    );

    // Verify that the provided contract-level output exists.
    for expected_contract in expected.contracts.iter() {
        let actual_contract = actual
            .contracts
            .get(expected_contract.path)
            .expect(&format!(
                "the file `{}` should exist",
                expected_contract.path
            ))
            .get(expected_contract.name)
            .expect(&format!(
                "the contract `{}` in file `{}` should exist",
                expected_contract.name, expected_contract.path
            ));
        let actual_contract_json = serde_json::to_value(actual_contract).unwrap();

        // Verify that every requested output exists (e.g. `evm.bytecode`).
        for field in expected_contract.fields.iter() {
            let mut parts = field.split('.');
            let (parent_field, child_field) = (parts.nth(0).unwrap(), parts.nth(1));
            let parent_output = actual_contract_json.get(parent_field);

            assert!(
                parent_output.is_some(),
                "the `{parent_field}` for the contract `{}` should be generated",
                expected_contract.name,
            );
            if let Some(child_field) = child_field {
                assert!(
                    parent_output.unwrap().get(child_field).is_some(),
                    "the `{child_field}` for the contract `{}` should be generated",
                    expected_contract.name,
                );
            }
        }

        // Verify that every non-requested output is omitted.
        for field in get_remaining_contract_fields(&expected_contract.fields) {
            let mut parts = field.split('.');
            let (parent_field, child_field) = (parts.nth(0).unwrap(), parts.nth(1));
            let parent_output = actual_contract_json.get(parent_field);

            if let Some(child_field) = child_field {
                assert!(
                    parent_output.is_none_or(|p| p.get(child_field).is_none()),
                    "the `{child_field}` for the contract `{}` should not be generated",
                    expected_contract.name,
                );
            } else {
                assert!(
                    parent_output.is_none(),
                    "the `{parent_field}` for the contract `{}` should not be generated",
                    expected_contract.name,
                );
            }
        }
    }
}

/// Gets the remaining JSON contract fields that are not in the given `fields_subset`.
fn get_remaining_contract_fields(fields_subset: &Vec<&'static str>) -> Vec<String> {
    SolcStandardJsonInputSettingsSelectionFileFlag::all()
        .iter()
        .filter(|flag| {
            let flag_string = serde_json::to_string(flag).unwrap();
            !fields_subset.contains(&&flag_string.as_str())
        })
        .map(|flag| serde_json::to_string(flag).unwrap())
        .collect()
}

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

#[test]
fn no_pvm_codegen_requested() {
    let files = &[
        STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH,
        STANDARD_JSON_NO_PVM_CODEGEN_ALL_WILDCARD_PATH,
    ];

    for file in files {
        let result = execute_resolc_with_stdin_input(&[JSON_OPTION], file);
        assert_command_success(&result, &format!("the `{file}` input fixture"));

        let output = to_solc_standard_json_output(&result.stdout);
        assert_no_errors(&output);

        let expected = ExpectedOutput {
            contracts: vec![
                ExpectedContract {
                    path: "lib/forge-std/src/interfaces/IERC165.sol",
                    name: "IERC165",
                    fields: vec!["abi", "evm.methodIdentifiers", "metadata"],
                },
                ExpectedContract {
                    path: "src/common/GasService.sol",
                    name: "GasService",
                    fields: vec!["abi", "evm.methodIdentifiers", "metadata"],
                },
                ExpectedContract {
                    path: "src/common/MessageDispatcher.sol",
                    name: "MessageDispatcher",
                    fields: vec!["abi", "evm.methodIdentifiers", "metadata"],
                },
            ],
        };
        assert_output_matches(&output, &expected);
    }
}

#[test]
fn pvm_codegen_requested() {
    let result = execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_CONTRACTS_PATH);
    assert_command_success(&result, "the PVM codegen std JSON input fixture");

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);

    let expected = ExpectedOutput {
        contracts: vec![
            ExpectedContract {
                path: "src/Counter.sol",
                name: "Counter",
                fields: vec!["evm.bytecode"],
            },
            ExpectedContract {
                path: "test/Counter.t.sol",
                name: "CounterTest",
                fields: vec!["evm.bytecode"],
            },
            ExpectedContract {
                path: "lib/forge-std/src/mocks/MockERC20.sol",
                name: "MockERC20",
                fields: vec!["evm.bytecode"],
            },
        ],
    };
    assert_output_matches(&output, &expected);
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
