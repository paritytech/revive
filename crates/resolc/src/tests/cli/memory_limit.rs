//! The tests for the deployment memory limit check.

use crate::cli_utils::{
    absolute_path, assert_command_failure, assert_command_success, execute_resolc,
    SOLIDITY_CONTRACT_PATH,
};

/// A heap buffer large enough to push the baseline interpreter memory over the Asset Hub budget.
const OVERSIZED_HEAP: &str = "2097152";

#[test]
fn compiles_within_the_default_limit() {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let result = execute_resolc(&[&path, "--bin"]);

    assert_command_success(&result, "A contract within the deployment limits");
}

#[test]
fn fails_when_the_contract_cannot_be_deployed() {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let result = execute_resolc(&[&path, "--bin", "--heap-size", OVERSIZED_HEAP]);

    assert_command_failure(&result, "A contract exceeding the runtime memory budget");
    assert!(
        result
            .stderr
            .contains("cannot be deployed to the target runtime"),
        "the error should explain that the contract is undeployable, got: {}",
        result.stderr
    );
}

#[test]
fn ignore_flag_downgrades_the_error_to_a_warning() {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let result = execute_resolc(&[
        &path,
        "--bin",
        "--heap-size",
        OVERSIZED_HEAP,
        "--ignore-memory-limit",
    ]);

    assert_command_success(
        &result,
        "An undeployable contract with the check downgraded",
    );
    assert!(
        result.stderr.contains("Warning"),
        "the violation should still be reported, got: {}",
        result.stderr
    );
}

#[test]
fn a_larger_budget_accepts_the_same_contract() {
    let path = absolute_path(SOLIDITY_CONTRACT_PATH);
    let result = execute_resolc(&[
        &path,
        "--bin",
        "--heap-size",
        OVERSIZED_HEAP,
        // Chosen above the 2233159 bytes the oversized heap needs.
        "--memory-limit",
        "4194304",
    ]);

    assert_command_success(&result, "An undeployable contract against a larger budget");
}
