//! The tests for running `resolc` with bin option.

use crate::{
    cli_utils::{
        absolute_path, assert_command_success, execute_command, CommandResult, ResolcOptSettings,
        SolcOptSettings, SOLIDITY_CONTRACT_PATH, STANDARD_JSON_CONTRACTS_PATH,
        YUL_MEMSET_CONTRACT_PATH,
    },
    SolcCompiler,
};

/// The starting hex value of a PVM blob (encoding of `"PVM"`).
const PVM_BLOB_START: &str = "50564d";

/// The starting hex value of an EVM blob compiled from Solidity.
const EVM_BLOB_START_FROM_SOLIDITY: &str = "6080";

/// The starting hex value of an EVM blob compiled from Yul.
/// (Blobs compiled from Yul do not have consistent starting hex values.)
const EVM_BLOB_START_FROM_YUL: &str = "";

/// Asserts that the `resolc` output contains a PVM blob.
fn assert_pvm_blob(result: &CommandResult) {
    assert_binary_blob(result, "Binary:\n", PVM_BLOB_START);
}

/// Asserts that the `solc` output from compiling Solidity contains an EVM blob.
fn assert_evm_blob_from_solidity(result: &CommandResult) {
    assert_binary_blob(result, "Binary:\n", EVM_BLOB_START_FROM_SOLIDITY);
}

/// Asserts that the `solc` output from compiling Yul contains an EVM blob.
fn assert_evm_blob_from_yul(result: &CommandResult) {
    assert_binary_blob(result, "Binary representation:\n", EVM_BLOB_START_FROM_YUL);
}

/// Asserts that the `resolc` output of compiling Solidity from JSON input contains a PVM blob.
/// - `result`: The result of running the command.
/// - `file_name`: The file name of the contract to verify the existence of a PVM blob for (corresponds to the name specified as a `source` in the JSON input).
/// - `contract_name`: The name of the contract to verify the existence of a PVM blob for.
fn assert_pvm_blob_from_json(result: &CommandResult, file_name: &str, contract_name: &str) {
    assert_binary_blob_from_json(result, file_name, contract_name, PVM_BLOB_START);
}

/// Asserts that the `solc` output of compiling Solidity from JSON input contains an EVM blob.
/// - `result`: The result of running the command.
/// - `file_name`: The file name of the contract to verify the existence of an EVM blob for (corresponds to the name specified as a `source` in the JSON input).
/// - `contract_name`: The name of the contract to verify the existence of an EVM blob for.
fn assert_evm_blob_from_json(result: &CommandResult, file_name: &str, contract_name: &str) {
    assert_binary_blob_from_json(
        result,
        file_name,
        contract_name,
        EVM_BLOB_START_FROM_SOLIDITY,
    );
}

/// Asserts that the output of a compilation contains a binary blob.
/// - `result`: The result of running the command.
/// - `blob_prefix`: The `stdout` message immediately preceding the binary blob representation.
/// - `blob_start`: The starting hex value of the binary blob.
fn assert_binary_blob(result: &CommandResult, blob_prefix: &str, blob_start: &str) {
    assert_command_success(result, "Executing the command");

    let is_blob = result
        .stdout
        .split(blob_prefix)
        .collect::<Vec<&str>>()
        .get(1)
        .is_some_and(|blob| blob.starts_with(blob_start));

    assert!(
        is_blob,
        "expected a binary blob starting with `{blob_start}`",
    );
}

/// Asserts that the output of compiling Solidity from JSON input contains a binary blob.
/// - `result`: The result of running the command.
/// - `file_name`: The file name of the contract to verify the existence of a binary blob for (corresponds to the name specified as a `source` in the JSON input).
/// - `contract_name`: The name of the contract to verify the existence of a binary blob for.
/// - `blob_start`: The starting hex value of the binary blob.
///
/// See [output description](https://docs.soliditylang.org/en/latest/using-the-compiler.html#output-description)
/// for more details on the JSON output format.
fn assert_binary_blob_from_json(
    result: &CommandResult,
    file_name: &str,
    contract_name: &str,
    blob_start: &str,
) {
    assert_command_success(result, "Executing the command with stdin JSON input");

    let parsed_output: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("expected valid JSON output");
    let contract = &parsed_output["contracts"][file_name][contract_name];

    let errors = contract["errors"].as_array();
    assert!(
        errors.is_none(),
        "errors found for JSON-provided contract `{contract_name}` in `{file_name}`: {}",
        get_first_json_error(errors.unwrap()),
    );

    let blob = contract["evm"]["bytecode"]["object"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "expected a binary blob for JSON-provided contract `{contract_name}` in `{file_name}`",
            )
        });
    assert!(
        blob.starts_with(blob_start),
        "expected a binary blob starting with `{blob_start}`",
    );
}

/// Gets the first error message reported when compiling from JSON input.
///
/// See [output description](https://docs.soliditylang.org/en/latest/using-the-compiler.html#output-description)
/// for more details on the JSON output format.
fn get_first_json_error(errors: &[serde_json::Value]) -> &str {
    errors.first().unwrap()["message"].as_str().unwrap()
}

/// This test mimics the command used in the `resolc` benchmarks when compiling Solidity.
#[test]
fn compiles_solidity_to_binary_blob() {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let resolc_arguments = &[&path, "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--bin",
        "--via-ir",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    let resolc_result = execute_command(crate::DEFAULT_EXECUTABLE_NAME, resolc_arguments, None);
    assert_pvm_blob(&resolc_result);

    let solc_result = execute_command(SolcCompiler::DEFAULT_EXECUTABLE_NAME, solc_arguments, None);
    assert_evm_blob_from_solidity(&solc_result);
}

/// This test mimics the command used in the `resolc` benchmarks when compiling Yul.
#[test]
fn compiles_yul_to_binary_blob() {
    let path = absolute_path(YUL_MEMSET_CONTRACT_PATH);
    let resolc_arguments = &[&path, "--yul", "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--strict-assembly",
        "--bin",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    let resolc_result = execute_command(crate::DEFAULT_EXECUTABLE_NAME, resolc_arguments, None);
    assert_pvm_blob(&resolc_result);

    let solc_result = execute_command(SolcCompiler::DEFAULT_EXECUTABLE_NAME, solc_arguments, None);
    assert_evm_blob_from_yul(&solc_result);
}

/// This test mimics the command used in the `resolc` benchmarks when compiling Solidity via standard JSON input.
#[test]
fn compiles_json_to_binary_blob() {
    let path = absolute_path(STANDARD_JSON_CONTRACTS_PATH);
    let resolc_arguments = &["--standard-json"];
    let solc_arguments = &["--standard-json"];

    let resolc_result = execute_command(
        crate::DEFAULT_EXECUTABLE_NAME,
        resolc_arguments,
        Some(&path),
    );
    assert_pvm_blob_from_json(&resolc_result, "src/Counter.sol", "Counter");

    let solc_result = execute_command(
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        solc_arguments,
        Some(&path),
    );
    assert_evm_blob_from_json(&solc_result, "src/Counter.sol", "Counter");
}
