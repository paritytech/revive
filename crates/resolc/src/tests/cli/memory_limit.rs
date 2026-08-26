//! The tests for the deployment memory limit check.

use revive_solc_json_interface::SolcStandardJsonOutput;

use crate::cli_utils::{
    absolute_path, assert_command_failure, assert_command_success, execute_resolc,
    execute_resolc_with_stdin_input, SOLIDITY_CONTRACT_PATH, STANDARD_JSON_OVERSIZED_HEAP_PATH,
};

/// A heap buffer large enough to push the baseline interpreter memory over the Asset Hub budget.
const OVERSIZED_HEAP: &str = "2097152";
/// Standard JSON always exits 0; undeployable contracts are reported in `errors`.
const JSON_OPTION: &str = "--standard-json";
const UNDEPLOYABLE: &str = "cannot be deployed to the target runtime";

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

#[test]
fn standard_json_fails_when_the_contract_cannot_be_deployed() {
    let result = execute_resolc_with_stdin_input(&[JSON_OPTION], STANDARD_JSON_OVERSIZED_HEAP_PATH);
    assert_command_success(
        &result,
        "Standard JSON reports the violation without failing the process",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert_errors_contain(&output, UNDEPLOYABLE);
}

#[test]
fn standard_json_ignore_flag_downgrades_the_error_to_a_warning() {
    let result = execute_resolc_with_stdin_input(
        &[JSON_OPTION, "--ignore-memory-limit"],
        STANDARD_JSON_OVERSIZED_HEAP_PATH,
    );
    assert_command_success(
        &result,
        "An undeployable contract with the check downgraded in standard JSON",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert_warnings_contain(&output, UNDEPLOYABLE);
    assert!(
        output.errors.iter().all(|error| !error.is_error()),
        "the ignore flag should leave no errors in the standard JSON output, got: {:?}",
        messages(&output)
    );
}

#[test]
fn standard_json_a_larger_budget_accepts_the_same_contract() {
    let result = execute_resolc_with_stdin_input(
        &[
            JSON_OPTION,
            // Chosen above the 2233159 bytes the oversized heap needs.
            "--memory-limit",
            "4194304",
        ],
        STANDARD_JSON_OVERSIZED_HEAP_PATH,
    );
    assert_command_success(
        &result,
        "An undeployable contract against a larger budget in standard JSON",
    );

    let output = to_solc_standard_json_output(&result.stdout);
    assert!(
        output.errors.iter().all(|error| !error.is_error()),
        "a larger budget should leave no errors in the standard JSON output, got: {:?}",
        messages(&output)
    );
}

fn to_solc_standard_json_output(json_text: &str) -> SolcStandardJsonOutput {
    serde_json::from_str(json_text)
        .unwrap_or_else(|error| panic!("standard JSON output should parse: {error}\n{json_text}"))
}

fn assert_errors_contain(output: &SolcStandardJsonOutput, message: &str) {
    assert!(
        output
            .errors
            .iter()
            .any(|error| error.is_error() && error.message.contains(message)),
        "the standard JSON output should contain the error `{message}`, got: {:?}",
        messages(output)
    );
}

fn assert_warnings_contain(output: &SolcStandardJsonOutput, message: &str) {
    assert!(
        output
            .errors
            .iter()
            .any(|error| error.is_warning() && error.message.contains(message)),
        "the standard JSON output should contain the warning `{message}`, got: {:?}",
        messages(output)
    );
}

fn messages(output: &SolcStandardJsonOutput) -> Vec<(&str, &str)> {
    output
        .errors
        .iter()
        .map(|error| (error.severity.as_str(), error.message.as_str()))
        .collect()
}
