//! The tests for running resolc with combined JSON option.

#![cfg(test)]

use revive_common;
use rstest::rstest;

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
fn runs_with_valid_argument() {
    for json_argument in JSON_ARGUMENTS {
        let arguments = &[utils::SOLIDITY_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = utils::execute_resolc(arguments);
        assert!(
            resolc_result.success,
            "Providing the `{json_argument}` argument should succeed with exit code {}, got {}.\nDetails: {}",
            revive_common::EXIT_CODE_SUCCESS,
            resolc_result.code,
            resolc_result.output
        );

        assert!(
            resolc_result.output.contains("contracts"),
            "Expected the output to contain a `contracts` field when using the `{json_argument}` argument."
        );

        let solc_result = utils::execute_solc(arguments);
        utils::assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}

#[test]
fn fails_with_invalid_argument() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        JSON_OPTION,
        "invalid-argument",
    ];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Providing an invalid json argument");

    assert!(
        resolc_result.output.contains("Invalid option"),
        "Expected the output to contain a specific error message."
    );

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_multiple_arguments() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        JSON_OPTION,
        JSON_ARGUMENTS[0],
        JSON_ARGUMENTS[1],
    ];
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Providing multiple json arguments");

    assert!(
        resolc_result
            .output
            .contains("reading error: No such file or directory"),
        "Expected the output to contain a specific error message."
    );

    // FIX: Resolc exit code == 101
    // let solc_result = utils::execute_solc(arguments);
    // utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[rstest]
#[case::exclude_input_file(&[JSON_OPTION])]
#[case::include_input_file(&[utils::SOLIDITY_CONTRACT_PATH, JSON_OPTION])]
fn fails_without_json_argument(#[case] arguments: &[&str]) {
    let resolc_result = utils::execute_resolc(arguments);
    utils::assert_command_failure(&resolc_result, "Omitting a JSON argument");

    assert!(
        resolc_result.output.contains(
            "a value is required for '--combined-json <COMBINED_JSON>' but none was supplied"
        ),
        "Expected the output to contain a specific error message."
    );

    let solc_result = utils::execute_solc(arguments);
    utils::assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_with_yul_input_file() {
    for json_argument in JSON_ARGUMENTS {
        let arguments = &[utils::YUL_CONTRACT_PATH, JSON_OPTION, json_argument];
        let resolc_result = utils::execute_resolc(arguments);
        utils::assert_command_failure(&resolc_result, "Providing a Yul input file");

        assert!(
            resolc_result
                .output
                .contains("ParserError: Expected identifier"),
            "Expected the output to contain a specific error message."
        );

        let solc_result = utils::execute_solc(arguments);
        utils::assert_equal_exit_codes(&solc_result, &resolc_result);
    }
}
