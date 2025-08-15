//! Common utilities used for CLI tests.

use std::process::{Command, Stdio};

use revive_common;

use crate::SolcCompiler;

pub const SOLIDITY_CONTRACT_PATH: &str = "src/tests/cli/contracts/solidity/contract.sol";
pub const YUL_CONTRACT_PATH: &str = "src/tests/cli/contracts/yul/contract.yul";
pub const YUL_MEMSET_CONTRACT_PATH: &str = "src/tests/cli/contracts/yul/memset.yul";

pub struct CommandResult {
    pub output: String,
    pub success: bool,
    pub code: i32,
}

pub fn execute_resolc(arguments: &[&str]) -> CommandResult {
    execute_command("resolc", arguments)
}

pub fn execute_solc(arguments: &[&str]) -> CommandResult {
    execute_command(SolcCompiler::DEFAULT_EXECUTABLE_NAME, arguments)
}

fn execute_command(command: &str, arguments: &[&str]) -> CommandResult {
    let result = Command::new(command)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    let output = if !result.stdout.is_empty() {
        result.stdout
    } else {
        result.stderr
    };

    CommandResult {
        output: String::from_utf8(output).unwrap(),
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

pub fn assert_command_failure(result: &CommandResult, error_message_prefix: &str) {
    assert!(
        !result.success,
        "{error_message_prefix} should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        result.code
    );
}
