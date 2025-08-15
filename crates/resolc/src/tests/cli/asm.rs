//! The tests for running resolc with asm option.

#![cfg(test)]

use revive_common;

use crate::tests::cli::utils;

const ASM_OPTION: &str = "--asm";

#[test]
fn can_run_with_valid_input_file() {
    const ARGUMENTS: &[&str] = &[utils::SOLIDITY_CONTRACT_PATH, ASM_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    assert!(
        resolc_result.success,
        "Providing a valid input file should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        resolc_result.code,
        resolc_result.output
    );

    for pattern in &["deploy", "call", "seal_return"] {
        assert!(
            resolc_result.output.contains(pattern),
            "Expected the output to contain `{}`.",
            pattern
        );
    }

    let solc_result = utils::execute_solc(ARGUMENTS);
    assert_eq!(
        solc_result.code, resolc_result.code,
        "Expected solc and resolc to have the same exit code."
    );
}

#[test]
fn fails_without_input_file() {
    const ARGUMENTS: &[&str] = &[ASM_OPTION];
    let resolc_result = utils::execute_resolc(ARGUMENTS);
    assert!(
        !resolc_result.success,
        "Omitting an input file should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        resolc_result.code
    );

    let output = resolc_result.output.to_lowercase();
    assert!(
        output.contains("no input sources specified") || output.contains("compilation aborted"),
        "Expected the output to contain a specific error message."
    );

    let solc_result = utils::execute_solc(ARGUMENTS);
    assert_eq!(
        solc_result.code, resolc_result.code,
        "Expected solc and resolc to have the same exit code."
    );
}
