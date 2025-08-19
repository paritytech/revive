//! Common utilities used for CLI tests.

use std::{
    fs::File,
    process::{Command, Stdio},
};

use revive_common;

use crate::SolcCompiler;

pub const SOLIDITY_CONTRACT_PATH: &str = "src/tests/cli/contracts/solidity/contract.sol";
pub const YUL_CONTRACT_PATH: &str = "src/tests/cli/contracts/yul/contract.yul";
pub const YUL_MEMSET_CONTRACT_PATH: &str = "src/tests/cli/contracts/yul/memset.yul";
pub const STANDARD_JSON_CONTRACTS_PATH: &str =
    "src/tests/cli/contracts/standard_json/solidity_contracts.json";

/// The result of executing a command.
pub struct CommandResult {
    /// The data written to `stdout`.
    pub stdout: String,
    /// The data written to `stderr`.
    pub stderr: String,
    /// Whether termination was successful.
    pub success: bool,
    /// The exit code of the process.
    pub code: i32,
}

pub fn execute_resolc(arguments: &[&str]) -> CommandResult {
    execute_command("resolc", arguments, None)
}

pub fn execute_resolc_with_stdin_input(arguments: &[&str], stdin_file_path: &str) -> CommandResult {
    execute_command("resolc", arguments, Some(stdin_file_path))
}

pub fn execute_solc(arguments: &[&str]) -> CommandResult {
    execute_command(SolcCompiler::DEFAULT_EXECUTABLE_NAME, arguments, None)
}

pub fn execute_solc_with_stdin_input(arguments: &[&str], stdin_file_path: &str) -> CommandResult {
    execute_command(
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        arguments,
        Some(stdin_file_path),
    )
}

fn execute_command(
    command: &str,
    arguments: &[&str],
    stdin_file_path: Option<&str>,
) -> CommandResult {
    let stdin_config = match stdin_file_path {
        Some(path) => Stdio::from(File::open(path).unwrap()),
        None => Stdio::null(),
    };
    let result = Command::new(command)
        .args(arguments)
        .stdin(stdin_config)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    CommandResult {
        stdout: String::from_utf8_lossy(&result.stdout).to_string(),
        stderr: String::from_utf8_lossy(&result.stderr).to_string(),
        success: result.status.success(),
        code: result.status.code().unwrap(),
    }
}

pub fn assert_equal_exit_codes(solc_result: &CommandResult, resolc_result: &CommandResult) {
    assert_eq!(
        solc_result.code, resolc_result.code,
        "Expected solc and resolc to have the same exit code."
    );
}

pub fn assert_command_success(result: &CommandResult, error_message_prefix: &str) {
    assert!(
        result.success,
        "{error_message_prefix} should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        result.code,
        result.stderr
    );
}

pub fn assert_command_failure(result: &CommandResult, error_message_prefix: &str) {
    assert!(
        !result.success,
        "{error_message_prefix} should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        result.code
    );
}
