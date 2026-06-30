//! The tests for running resolc with standard JSON option.

use revive_solc_json_interface::{
    PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize,
    SolcStandardJsonInputSettingsSelectionFileFlag, SolcStandardJsonOutput,
};

use crate::cli_utils::{
    assert_command_success, assert_equal_exit_codes, execute_resolc_with_stdin_input,
    execute_solc_with_stdin_input, STANDARD_JSON_ALL_OUTPUTS_PATH, STANDARD_JSON_CONTRACTS_PATH,
    STANDARD_JSON_NEWYORK_DISABLED_PATH, STANDARD_JSON_NEWYORK_ENABLED_PATH,
    STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH, STANDARD_JSON_NO_EVM_CODEGEN_PATH,
    STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH, STANDARD_JSON_PVM_CODEGEN_ALL_WILDCARD_PATH,
    STANDARD_JSON_PVM_CODEGEN_ONE_FILE_PATH, STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH,
    STANDARD_JSON_YUL_NEWYORK_DISABLED_PATH, STANDARD_JSON_YUL_NEWYORK_ENABLED_PATH,
    STANDARD_JSON_YUL_NO_PVM_CODEGEN_PATH, STANDARD_JSON_YUL_PVM_CODEGEN_PATH,
};
use crate::{pipeline_name, ResolcVersion};

const JSON_OPTION: &str = "--standard-json";

/// A subset of contracts and sources expected to exist in the JSON output.
struct ExpectedOutput {
    contracts: Vec<ExpectedContract>,
    sources: Vec<ExpectedSource>,
}

/// A contract expected to exist in the JSON output.
struct ExpectedContract {
    /// The file path.
    path: &'static str,
    /// The contract name.
    name: &'static str,
    /// All contract-level fields of the JSON output selection expected to exist,
    /// such as `metadata`, `irOptimized`, etc. If `evm.bytecode` was requested,
    /// both `evm` and `evm.bytecode` should be expected.
    fields: Vec<&'static str>,
}

/// A source expected to exist in the JSON output.
struct ExpectedSource {
    /// The file path.
    path: &'static str,
    /// All file-level fields of the JSON output selection expected to exist,
    /// such as `ast`, as well as the `id` field.
    fields: Vec<&'static str>,
}

/// Asserts that the `expected` subset of contracts and sources match the ones in the `actual` output.
/// If expected sources or contracts are empty, asserts that the respective actual output is also empty.
fn assert_output_matches(actual: &SolcStandardJsonOutput, expected: &ExpectedOutput) {
    assert_sources_output_matches(actual, expected);
    assert_contracts_output_matches(actual, expected);
}

/// Asserts that the `expected` subset of sources match the ones in the `actual` output.
/// If expected sources is empty, asserts that the actual output is also empty.
fn assert_sources_output_matches(actual: &SolcStandardJsonOutput, expected: &ExpectedOutput) {
    if expected.sources.is_empty() {
        assert!(actual.sources.is_empty(), "sources should not be populated");
        return;
    }

    assert!(
        actual.sources.len() >= expected.sources.len(),
        "at least {} sources should be populated",
        expected.sources.len()
    );

    for expected_source in &expected.sources {
        let actual_source = actual
            .sources
            .get(expected_source.path)
            .unwrap_or_else(|| panic!("the file `{}` should exist", expected_source.path));
        let actual_source_json = serde_json::to_value(actual_source).unwrap();

        // Verify that every expected output exists.
        for field in &expected_source.fields {
            assert!(
                actual_source_json.get(field).is_some(),
                "the `{field}` for the file `{}` should be generated",
                expected_source.path
            );
        }

        // Verify that every unexpected output is omitted.
        let all_fields = &["id", "ast"];
        for field in all_fields {
            if !expected_source.fields.contains(field) {
                assert!(
                    actual_source_json.get(field).is_none(),
                    "the `{field}` for the file `{}` should not be generated",
                    expected_source.path
                );
            }
        }
    }
}

/// Asserts that the `expected` subset of contracts match the ones in the `actual` output.
/// If expected contracts is empty, asserts that the actual output is also empty.
fn assert_contracts_output_matches(actual: &SolcStandardJsonOutput, expected: &ExpectedOutput) {
    if expected.contracts.is_empty() {
        assert!(
            actual.contracts.is_empty(),
            "contracts should not be populated"
        );
        return;
    }

    assert!(
        actual.contracts.len() >= expected.contracts.len(),
        "at least {} contracts should be populated",
        expected.contracts.len()
    );

    for expected_contract in &expected.contracts {
        let actual_contract = actual
            .contracts
            .get(expected_contract.path)
            .unwrap_or_else(|| panic!("the file `{}` should exist", expected_contract.path))
            .get(expected_contract.name)
            .unwrap_or_else(|| {
                panic!(
                    "the contract `{}` in file `{}` should exist",
                    expected_contract.name, expected_contract.path
                )
            });
        let actual_contract_json = serde_json::to_value(actual_contract).unwrap();

        // Verify that every expected output exists (e.g. `evm.bytecode`).
        for field in &expected_contract.fields {
            let mut parts = field.split('.');
            let (parent_field, child_field) = (parts.next().unwrap(), parts.next());
            let parent_output = actual_contract_json.get(parent_field);

            assert!(
                parent_output.is_some(),
                "the `{parent_field}` for the contract `{}` should be generated",
                expected_contract.name,
            );
            if let Some(child_field) = child_field {
                assert!(
                    parent_output.unwrap().get(child_field).is_some(),
                    "the `{field}` for the contract `{}` should be generated",
                    expected_contract.name,
                );
            }
        }

        // Verify that every unexpected output is omitted.
        for flag in SolcStandardJsonInputSettingsSelectionFileFlag::all() {
            let field = serde_json::to_string(flag)
                .unwrap()
                .trim_matches('"')
                .to_owned();
            if !expected_contract.fields.contains(&field.as_str()) {
                let mut parts = field.split('.');
                let (parent_field, child_field) = (parts.next().unwrap(), parts.next());
                let parent_output = actual_contract_json.get(parent_field);

                if let Some(child_field) = child_field {
                    assert!(
                        parent_output.is_none_or(|p| p.get(child_field).is_none()),
                        "the `{field}` for the contract `{}` should not be generated",
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
    let mut has_errors = false;
    for error in output.errors.iter().filter(|error| error.is_error()) {
        eprintln!(
            "ERROR in {}: {}",
            error
                .source_location
                .as_ref()
                .map(|l| l.file.as_str())
                .unwrap_or("unknown"),
            error.message,
        );
        has_errors = true;
    }
    assert!(
        !has_errors,
        "the standard JSON output should not contain errors"
    );
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
fn all_outputs_requested() {
    let result = execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_ALL_OUTPUTS_PATH);
    assert_command_success(&result, "the all output std JSON input fixture");

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);

    let expected_contract_fields = &[
        "abi",
        "metadata",
        "devdoc",
        "userdoc",
        "storageLayout",
        "irOptimized",
        "ir",
        "evm",
        "evm.methodIdentifiers",
        "evm.bytecode",
        "evm.deployedBytecode",
        "evm.assembly",
    ];
    let expected = ExpectedOutput {
        contracts: vec![
            ExpectedContract {
                path: "src/Counter.sol",
                name: "Counter",
                fields: expected_contract_fields.into(),
            },
            ExpectedContract {
                path: "script/Counter.s.sol",
                name: "CounterScript",
                fields: expected_contract_fields.into(),
            },
            ExpectedContract {
                path: "lib/forge-std/src/mocks/MockERC20.sol",
                name: "MockERC20",
                fields: expected_contract_fields.into(),
            },
        ],
        sources: vec![
            ExpectedSource {
                path: "src/Counter.sol",
                fields: vec!["id", "ast"],
            },
            ExpectedSource {
                path: "script/Counter.s.sol",
                fields: vec!["id", "ast"],
            },
            ExpectedSource {
                path: "lib/forge-std/src/mocks/MockERC20.sol",
                fields: vec!["id", "ast"],
            },
        ],
    };
    assert_output_matches(&output, &expected);
}

#[test]
fn pvm_codegen_requested() {
    let files = &[
        STANDARD_JSON_PVM_CODEGEN_ALL_WILDCARD_PATH,
        STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH,
        STANDARD_JSON_PVM_CODEGEN_ONE_FILE_PATH,
    ];

    for file in files {
        let result = execute_resolc_with_stdin_input(&[JSON_OPTION], file);
        assert_command_success(&result, &format!("the `{file}` input fixture"));

        let output = to_solc_standard_json_output(&result.stdout);
        assert_no_errors(&output);

        let requests_codegen_for_one_file = *file == STANDARD_JSON_PVM_CODEGEN_ONE_FILE_PATH;
        let expected_contract_fields_with_codegen = &[
            "abi",
            "metadata",
            "evm",
            "evm.bytecode",
            "evm.methodIdentifiers",
        ];
        let expected_contract_fields_without_codegen =
            &["abi", "metadata", "evm", "evm.methodIdentifiers"];

        let expected = ExpectedOutput {
            contracts: vec![
                ExpectedContract {
                    path: "src/common/GasService.sol",
                    name: "GasService",
                    // This is the contract expected to have codegen for all files tested here.
                    fields: expected_contract_fields_with_codegen.into(),
                },
                ExpectedContract {
                    path: "src/common/Gateway.sol",
                    name: "Gateway",
                    fields: if requests_codegen_for_one_file {
                        expected_contract_fields_without_codegen.into()
                    } else {
                        expected_contract_fields_with_codegen.into()
                    },
                },
                ExpectedContract {
                    path: "src/common/MessageDispatcher.sol",
                    name: "MessageDispatcher",
                    fields: if requests_codegen_for_one_file {
                        expected_contract_fields_without_codegen.into()
                    } else {
                        expected_contract_fields_with_codegen.into()
                    },
                },
            ],
            sources: vec![
                ExpectedSource {
                    path: "src/common/GasService.sol",
                    fields: vec!["id"],
                },
                ExpectedSource {
                    path: "src/common/Gateway.sol",
                    fields: vec!["id"],
                },
                ExpectedSource {
                    path: "src/common/MessageDispatcher.sol",
                    fields: vec!["id"],
                },
            ],
        };
        assert_output_matches(&output, &expected);
    }
}

#[test]
fn no_pvm_codegen_requested() {
    let result =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH);
    assert_command_success(
        &result,
        "the no PVM codegen std JSON per file input fixture",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);

    let expected = ExpectedOutput {
        contracts: vec![
            ExpectedContract {
                path: "src/common/GasService.sol",
                name: "GasService",
                fields: vec!["abi", "evm", "evm.methodIdentifiers", "metadata"],
            },
            ExpectedContract {
                path: "src/common/Gateway.sol",
                name: "Gateway",
                fields: vec!["abi", "evm", "evm.methodIdentifiers", "metadata"],
            },
            ExpectedContract {
                path: "src/common/MessageDispatcher.sol",
                name: "MessageDispatcher",
                fields: vec!["abi", "evm", "evm.methodIdentifiers", "metadata"],
            },
        ],
        sources: vec![
            ExpectedSource {
                path: "src/common/GasService.sol",
                fields: vec!["id"],
            },
            ExpectedSource {
                path: "src/common/Gateway.sol",
                fields: vec!["id"],
            },
            ExpectedSource {
                path: "src/common/MessageDispatcher.sol",
                fields: vec!["id"],
            },
        ],
    };
    assert_output_matches(&output, &expected);
}

#[test]
fn yul_pvm_codegen_requested() {
    let result =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_YUL_PVM_CODEGEN_PATH);
    assert_command_success(&result, "the PVM codegen from Yul std JSON input fixture");

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);

    let expected = ExpectedOutput {
        contracts: vec![ExpectedContract {
            path: "Test",
            name: "Return",
            fields: vec![
                "evm",
                "evm.bytecode",
                "evm.deployedBytecode",
                "evm.assembly",
            ],
        }],
        sources: vec![],
    };
    assert_output_matches(&output, &expected);
}

#[test]
fn yul_no_pvm_codegen_requested() {
    let result =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_YUL_NO_PVM_CODEGEN_PATH);
    assert_command_success(
        &result,
        "the no PVM codegen from Yul std JSON input fixture",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert_no_errors(&output);

    let expected = ExpectedOutput {
        contracts: vec![],
        sources: vec![],
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
        TestCase {
            arguments: vec![JSON_OPTION, "--newyork"],
            error_message:
                "The newyork IR pipeline must be enabled in standard JSON input polkavm settings",
        },
    ];

    for case in cases {
        let result = execute_resolc_with_stdin_input(&case.arguments, STANDARD_JSON_CONTRACTS_PATH);
        let output = to_solc_standard_json_output(&result.stdout);
        assert_standard_json_errors_contain(&output, case.error_message);
    }
}

/// Extracts the hex PVM bytecode object of `path`/`name` from a standard JSON output.
fn bytecode_object(output: &SolcStandardJsonOutput, path: &str, name: &str) -> String {
    let contract = output
        .contracts
        .get(path)
        .and_then(|file| file.get(name))
        .unwrap_or_else(|| panic!("the contract `{name}` in `{path}` should exist"));
    serde_json::to_value(contract)
        .unwrap()
        .get("evm")
        .and_then(|evm| evm.get("bytecode"))
        .and_then(|bytecode| bytecode.get("object"))
        .and_then(|object| object.as_str())
        .expect("the bytecode object should be present")
        .to_owned()
}

/// The `settings.polkavm.newyork` standard JSON input field selects the newyork
/// pipeline: for the same source it yields different bytecode than the stock
/// pipeline, and the field is off by default (the disabled fixture omits it).
#[test]
fn pvm_codegen_newyork_input_setting() {
    let enabled =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NEWYORK_ENABLED_PATH);
    assert_command_success(
        &enabled,
        "the newyork-enabled standard JSON input should build",
    );
    let disabled =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_NEWYORK_DISABLED_PATH);
    assert_command_success(&disabled, "the stock standard JSON input should build");

    let enabled_output = to_solc_standard_json_output(&enabled.stdout);
    let disabled_output = to_solc_standard_json_output(&disabled.stdout);
    assert_no_errors(&enabled_output);
    assert_no_errors(&disabled_output);

    assert_ne!(
        bytecode_object(&enabled_output, "C.sol", "C"),
        bytecode_object(&disabled_output, "C.sol", "C"),
        "settings.polkavm.newyork should select a different pipeline than the default"
    );
}

/// `settings.polkavm.newyork` also selects the newyork pipeline for `"language": "Yul"`
/// standard JSON input: the same Yul source yields different bytecode than the stock pipeline.
#[test]
fn pvm_codegen_newyork_yul_input() {
    let enabled =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_YUL_NEWYORK_ENABLED_PATH);
    assert_command_success(
        &enabled,
        "the newyork-enabled Yul standard JSON input should build",
    );
    let disabled =
        execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_YUL_NEWYORK_DISABLED_PATH);
    assert_command_success(&disabled, "the stock Yul standard JSON input should build");

    let enabled_output = to_solc_standard_json_output(&enabled.stdout);
    let disabled_output = to_solc_standard_json_output(&disabled.stdout);
    assert_no_errors(&enabled_output);
    assert_no_errors(&disabled_output);

    assert_ne!(
        bytecode_object(&enabled_output, "Test", "Test"),
        bytecode_object(&disabled_output, "Test", "Test"),
        "settings.polkavm.newyork should select a different pipeline for Yul input"
    );
}

#[test]
fn populates_output_metadata_fields() {
    for (path, use_newyork) in [
        (STANDARD_JSON_CONTRACTS_PATH, false),
        (STANDARD_JSON_NEWYORK_ENABLED_PATH, true),
        (STANDARD_JSON_NEWYORK_DISABLED_PATH, false),
    ] {
        let result = execute_resolc_with_stdin_input(&[JSON_OPTION], path);
        assert_command_success(&result, "Compiling with standard JSON");

        let output = to_solc_standard_json_output(&result.stdout);
        assert_no_errors(&output);

        assert_eq!(
            output.revive_version.as_deref(),
            Some(ResolcVersion::default().long.as_str()),
            "Standard JSON output for `{path}` should populate `revive_version`"
        );
        assert_eq!(
            output.resolc_pipeline.as_deref(),
            Some(pipeline_name(use_newyork)),
            "Standard JSON output for `{path}` should report the correct `resolc_pipeline`"
        );
        assert!(
            output.version.is_some(),
            "Standard JSON output for `{path}` should populate `version`"
        );
        assert!(
            output.long_version.is_some(),
            "Standard JSON output for `{path}` should populate `long_version`"
        );
    }
}
