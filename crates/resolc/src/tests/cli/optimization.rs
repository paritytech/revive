//! The tests for running resolc with explicit optimization.

#![cfg(test)]

use revive_common;

use crate::tests::cli::{utils, yul};

const LEVELS: &[char] = &['0', '1', '2', '3', 's', 'z'];

#[test]
fn runs_with_valid_level() {
    for level in LEVELS {
        let optimization_argument = format!("-O{level}");
        let arguments = &[
            utils::YUL_MEMSET_CONTRACT_PATH,
            yul::YUL_OPTION,
            &optimization_argument,
        ];
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
