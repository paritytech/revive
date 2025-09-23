//! The tests for running resolc with combined JSON option.

#![cfg(test)]

use revive_common;
use revive_solc_json_interface::CombinedJsonInvalidSelectorMessage;

use crate::tests::cli::utils;

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
        let arguments = &[utils::SOLIDITY_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = utils::execute_resolc(arguments);
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

        let solc_result = utils::execute_solc(arguments);
        utils::assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}

#[test]
fn fails_with_invalid_json_argument() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        JSON_OPTION,
        "invalid-argument",
    ];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Providing an invalid json argument");

    assert!(resolc_result
        .stderr
        .contains(CombinedJsonInvalidSelectorMessage));

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_multiple_json_arguments() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        JSON_OPTION,
        JSON_ARGUMENTS[0],
        JSON_ARGUMENTS[1],
    ];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Providing multiple json arguments");

    assert!(resolc_result
        .stderr
        .contains(&format!("Error: \"{}\" is not found.", JSON_ARGUMENTS[1])),);

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_json_argument() {
    let arguments = &[utils::SOLIDITY_CONTRACT_PATH, JSON_OPTION];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Omitting a JSON argument");

    assert!(resolc_result.stderr.contains(
        "a value is required for '--combined-json <COMBINED_JSON>' but none was supplied"
    ));

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_solidity_input_file() {
    let arguments = &[JSON_OPTION, JSON_ARGUMENTS[0]];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Omitting a Solidity input file");

    assert!(resolc_result.stderr.contains("Error: No input files given"),);

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_yul_input_file() {
    for json_argument in JSON_ARGUMENTS {
        let arguments = &[utils::YUL_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = utils::execute_resolc(arguments);
        utils::assert_command_failure(&resolc_result, "Providing a Yul input file");

        assert!(resolc_result
            .stderr
            .contains("Error: Expected identifier but got 'StringLiteral'"));

        let solc_result = utils::execute_solc(arguments);
        utils::assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}
