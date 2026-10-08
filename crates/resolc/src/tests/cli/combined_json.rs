//! The tests for running resolc with combined JSON option.

use revive_solc_json_interface::{combined_json::CombinedJson, CombinedJsonInvalidSelectorMessage};

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_resolc_in_directory, execute_solc, SOLIDITY_CONTRACT_PATH,
    SOLIDITY_INCLUDED_IMPORT_PATH, SOLIDITY_REMAPPED_IMPORT_PATH,
    SOLIDITY_REMAPPINGS_ORDER_FIRST_DIRECTORY, SOLIDITY_REMAPPINGS_ORDER_MAIN_PATH,
    SOLIDITY_REMAPPINGS_ORDER_SECOND_DIRECTORY, SOLIDITY_TWO_CONTRACTS_PATH, YUL_CONTRACT_PATH,
};
use crate::ResolcVersion;

const JSON_OPTION: &str = "--combined-json";
const JSON_ARGUMENTS: &[&str] = &[
    "abi",
    "hashes",
    "metadata",
    "devdoc",
    "userdoc",
    "storage-layout",
    "ast",
    "asm",
    "bin",
    "bin-runtime",
];

#[test]
fn runs_with_valid_json_argument() {
    for json_argument in JSON_ARGUMENTS {
        let arguments = &[SOLIDITY_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = execute_resolc(arguments);
        assert!(
            resolc_result.success,
            "Providing the `{json_argument}` argument should succeed with exit code {}, got {}.\nDetails: {}",
            revive_common::EXIT_CODE_SUCCESS,
            resolc_result.code,
            resolc_result.stderr
        );

        assert!(
            resolc_result.stdout.contains("contracts"),
            "Expected the output to contain a `contracts` field when using the `{json_argument}` argument."
        );

        let solc_result = execute_solc(arguments);
        assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}

#[test]
fn fails_with_invalid_json_argument() {
    let arguments = &[SOLIDITY_CONTRACT_PATH, JSON_OPTION, "invalid-argument"];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Providing an invalid json argument");

    assert!(resolc_result
        .stderr
        .contains(CombinedJsonInvalidSelectorMessage));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_multiple_json_arguments() {
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        JSON_OPTION,
        JSON_ARGUMENTS[0],
        JSON_ARGUMENTS[1],
    ];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Providing multiple json arguments");

    assert!(resolc_result
        .stderr
        .contains(&format!("Error: \"{}\" is not found.", JSON_ARGUMENTS[1])),);

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_json_argument() {
    let arguments = &[SOLIDITY_CONTRACT_PATH, JSON_OPTION];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Omitting a JSON argument");

    assert!(resolc_result.stderr.contains(
        "a value is required for '--combined-json <COMBINED_JSON>' but none was supplied"
    ));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_solidity_input_file() {
    let arguments = &[JSON_OPTION, JSON_ARGUMENTS[0]];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Omitting a Solidity input file");

    assert!(resolc_result.stderr.contains("Error: No input files given"),);

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_yul_input_file() {
    for json_argument in JSON_ARGUMENTS {
        let arguments = &[YUL_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = execute_resolc(arguments);
        assert_command_failure(&resolc_result, "Providing a Yul input file");

        assert!(resolc_result
            .stderr
            .contains("Error: Expected identifier but got 'StringLiteral'"));

        let solc_result = execute_solc(arguments);
        assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}

#[test]
fn populates_output_metadata_fields() {
    let arguments = &[SOLIDITY_CONTRACT_PATH, JSON_OPTION, JSON_ARGUMENTS[0]];
    let result = execute_resolc(arguments);
    assert_command_success(&result, "Compiling with combined JSON");

    let combined_json: CombinedJson =
        serde_json::from_str(&result.stdout).expect("Combined JSON output should deserialize");
    assert_eq!(
        combined_json.resolc_version.as_deref(),
        Some(ResolcVersion::default().long.as_str()),
        "Combined JSON output should populate `resolc_version`"
    );
    assert!(
        !combined_json.version.is_empty(),
        "Combined JSON output should populate `version`"
    );
}

/// Asserts that resolc and solc both emit `expected` contracts for `arguments`.
fn assert_combined_json_contracts(arguments: &[&str], expected: &[&str]) {
    let resolc_result = execute_resolc(arguments);
    assert_command_success(&resolc_result, "Compiling with combined JSON");

    let combined_json: CombinedJson = serde_json::from_str(&resolc_result.stdout)
        .expect("Combined JSON output should deserialize");
    let contracts = combined_json
        .contracts
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(contracts, expected);

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn resolves_imports_with_remappings() {
    let arguments = &[
        "@solidity/=src/tests/data/solidity/",
        SOLIDITY_REMAPPED_IMPORT_PATH,
        JSON_OPTION,
        "abi,bin",
    ];
    assert_combined_json_contracts(
        arguments,
        &[
            "src/tests/data/solidity/contract.sol:C",
            "src/tests/data/solidity/remapped_import.sol:U",
        ],
    );
}

/// The last of two remappings for the same prefix wins, as in solc.
#[test]
fn remappings_last_one_wins() {
    let first_remapping = format!("target/={SOLIDITY_REMAPPINGS_ORDER_FIRST_DIRECTORY}");
    let second_remapping = format!("target/={SOLIDITY_REMAPPINGS_ORDER_SECOND_DIRECTORY}");
    let arguments = &[
        first_remapping.as_str(),
        second_remapping.as_str(),
        SOLIDITY_REMAPPINGS_ORDER_MAIN_PATH,
        JSON_OPTION,
        "abi",
    ];
    let result = execute_resolc(arguments);
    assert_command_success(&result, "Compiling with combined JSON");

    let combined_json: CombinedJson =
        serde_json::from_str(&result.stdout).expect("Combined JSON output should deserialize");
    let main = &combined_json.contracts[&format!("{SOLIDITY_REMAPPINGS_ORDER_MAIN_PATH}:Main")];
    assert_eq!(main.abi[0]["name"], "second");
}

#[test]
fn resolves_imports_with_base_and_include_paths() {
    let arguments = &[
        SOLIDITY_INCLUDED_IMPORT_PATH,
        "--base-path",
        ".",
        "--include-path",
        "src/tests/data",
        JSON_OPTION,
        "abi,bin",
    ];
    assert_combined_json_contracts(
        arguments,
        &[
            "solidity/contract.sol:C",
            "src/tests/data/solidity/included_import.sol:U",
        ],
    );
}

/// Contracts in a source whose path contains a colon keep their own entries.
#[test]
fn keeps_contracts_of_source_path_with_colon_apart() {
    const SOURCE_PATH_WITH_COLON: &str = "directory:file.sol";

    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        SOLIDITY_TWO_CONTRACTS_PATH,
        directory.path().join(SOURCE_PATH_WITH_COLON),
    )
    .unwrap();

    let result = execute_resolc_in_directory(
        &[SOURCE_PATH_WITH_COLON, JSON_OPTION, "bin"],
        directory.path(),
    );
    assert_command_success(&result, "Compiling a source path with a colon");

    let combined_json: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("Combined JSON output should deserialize");
    let bytecode = |name: &str| {
        combined_json["contracts"][format!("{SOURCE_PATH_WITH_COLON}:{name}")]["bin"]
            .as_str()
            .unwrap_or_else(|| panic!("Combined JSON output should contain `{name}` bytecode"))
            .to_owned()
    };
    let first = bytecode("First");
    let second = bytecode("Second");
    assert!(!first.is_empty() && !second.is_empty());
    assert_ne!(first, second);
}
