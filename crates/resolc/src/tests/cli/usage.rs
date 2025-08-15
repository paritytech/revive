//! The tests for running resolc when expecting usage output.

#![cfg(test)]

use revive_common;

use crate::tests::cli::utils;

#[test]
fn fails_without_options() {
    let resolc_result = utils::execute_resolc(&[]);
    assert!(
        !resolc_result.success,
        "Omitting options should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        resolc_result.code
    );

    assert!(
        resolc_result.output.contains("Usage: resolc"),
        "Expected the output to contain usage information."
    );

    let solc_result = utils::execute_solc(&[]);
    assert_eq!(
        solc_result.code, resolc_result.code,
        "Expected solc and resolc to have the same exit code."
    );
}
