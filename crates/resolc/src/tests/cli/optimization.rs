//! The tests for running resolc with explicit optimization.

use crate::tests::cli::utils::{
    self, assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc, RESOLC_YUL_FLAG, SOLIDITY_CONTRACT_PATH, SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH,
    YUL_MEMSET_CONTRACT_PATH,
};

const LEVELS: &[char] = &['0', '1', '2', '3', 's', 'z'];

#[test]
fn runs_with_valid_level() {
    for level in LEVELS {
        let optimization_argument = format!("-O{level}");
        let arguments = &[YUL_MEMSET_CONTRACT_PATH, "--yul", &optimization_argument];
        let resolc_result = utils::execute_resolc(arguments);
        assert!(
            resolc_result.success,
            "Providing the level `{optimization_argument}` should succeed with exit code {}, got {}.\nDetails: {}",
            revive_common::EXIT_CODE_SUCCESS,
            resolc_result.code,
            resolc_result.stderr
        );

        assert!(
            resolc_result
                .stderr
                .contains("Compiler run successful. No output requested"),
            "Expected the output to contain a success message when providing the level `{optimization_argument}`."
        );
    }
}

#[test]
fn fails_with_invalid_level() {
    let arguments = &[YUL_MEMSET_CONTRACT_PATH, RESOLC_YUL_FLAG, "-O9"];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(&resolc_result, "Providing an invalid optimization level");

    assert!(resolc_result
        .stderr
        .contains("Unexpected optimization option"));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn disable_solc_optimzer() {
    let arguments = &[SOLIDITY_CONTRACT_PATH, "--bin", "--disable-solc-optimizer"];
    let disabled = execute_resolc(arguments);
    assert_command_success(&disabled, "Disabling the solc optimizer");

    let arguments = &[SOLIDITY_CONTRACT_PATH, "--bin"];
    let enabled = execute_resolc(arguments);
    assert_command_success(&disabled, "Enabling the solc optimizer");

    assert_ne!(enabled.stdout, disabled.stdout);
}

#[test]
fn test_large_div_rem_expansion() {
    for level in LEVELS {
        let optimization_argument = format!("-O{level}");
        let arguments = &[SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH, &optimization_argument];
        let resolc_result = utils::execute_resolc(arguments);
        assert!(
            resolc_result.success,
            "Providing the level `{optimization_argument}` should succeed with exit code {}, got {}.\nDetails: {}",
            revive_common::EXIT_CODE_SUCCESS,
            resolc_result.code,
            resolc_result.stderr
        );

        assert!(
            resolc_result
                .stderr
                .contains("Compiler run successful. No output requested"),
            "Expected the output to contain a success message when providing the level `{optimization_argument}`."
        );
    }
}
