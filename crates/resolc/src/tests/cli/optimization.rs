//! The tests for running resolc with explicit optimization.

#![cfg(test)]

use revive_common;
use rstest::rstest;

use crate::tests::cli::{utils, yul};

#[rstest]
#[case::level_0('0')]
#[case::level_1('1')]
#[case::level_2('2')]
#[case::level_3('3')]
#[case::level_s('s')]
#[case::level_z('z')]
fn runs_with_valid_level(#[case] level: char) {
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
        resolc_result.output
    );

    assert!(
        resolc_result
            .output
            .contains("Compiler run successful. No output requested"),
        "Expected the output to contain a success message."
    );
}
