//! Common utilities used for CLI tests.

use std::{
    fs::File,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::SolcCompiler;

/// The simple Solidity contract test fixture path.
pub const SOLIDITY_CONTRACT_PATH: &str = "src/tests/data/solidity/contract.sol";
/// The dependency Solidity contract test fixture path.
pub const SOLIDITY_DEPENDENCY_CONTRACT_PATH: &str = "src/tests/data/solidity/dependency.sol";
/// The simple Solidity contract containing i256 divisions and remains
/// that should be compiled correctly.
pub const SOLIDITY_LARGE_DIV_REM_CONTRACT_PATH: &str = "src/tests/data/solidity/large_div_rem.sol";

/// The simple YUL contract test fixture path.
pub const YUL_CONTRACT_PATH: &str = "src/tests/data/yul/contract.yul";
/// The memeset YUL contract test fixture path.
pub const YUL_MEMSET_CONTRACT_PATH: &str = "src/tests/data/yul/memset.yul";
/// The return YUL contract test fixture path.
pub const YUL_RETURN_CONTRACT_PATH: &str = "src/tests/data/yul/return.yul";

/// The standard JSON contracts test fixture path.
pub const STANDARD_JSON_CONTRACTS_PATH: &str =
    "src/tests/data/standard_json/solidity_contracts.json";
/// The standard JSON contracts test fixture path that requests every single output.
pub const STANDARD_JSON_ALL_OUTPUT_PATH: &str = "src/tests/data/standard_json/all_output.json";
/// The standard JSON no EVM codegen test fixture path.
///
/// This contains EVM bytecode selection flags with provided code
/// that doesn't compile without `viaIr`. Because we remove those
/// selection flags, it should compile fine regardless.
pub const STANDARD_JSON_NO_EVM_CODEGEN_PATH: &str =
    "src/tests/data/standard_json/no_evm_codegen.json";
/// This is a complex contract from a real dApp, triggering the
/// infamous "Stack too deep" error in the EVM codegen.
pub const STANDARD_JSON_NO_EVM_CODEGEN_COMPLEX_PATH: &str =
    "src/tests/data/standard_json/no_evm_codegen_complex.json";
/// The standard JSON PVM codegen all wildcard test fixture path.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_PVM_CODEGEN_ALL_WILDCARD_PATH: &str =
    "src/tests/data/standard_json/pvm_codegen_all_wildcard.json";
/// The standard JSON PVM codegen per file test fixture path.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH: &str =
    "src/tests/data/standard_json/pvm_codegen_per_file.json";
/// The standard JSON no PVM codegen test fixture path with
/// lots of files included.
///
/// This omits `evm` bytecode selection flags, which should thereby
/// prevent PVM bytecode generation.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_NO_PVM_CODEGEN_MANY_FILES_PATH: &str =
    "src/tests/data/standard_json/no_pvm_codegen_many_files.json";
/// The standard JSON Yul contract PVM codegen test fixture path.
///
/// This requests the full `evm` output object, which should thereby
/// generate all `evm` child fields.
pub const STANDARD_JSON_YUL_PVM_CODEGEN_PATH: &str =
    "src/tests/data/standard_json/yul_pvm_codegen.json";
/// The standard JSON Yul contract no PVM codegen test fixture path.
///
/// This omits `evm` bytecode selection flags, which should thereby prevent
/// PVM bytecode generation and only validate the Yul.
pub const STANDARD_JSON_YUL_NO_PVM_CODEGEN_PATH: &str =
    "src/tests/data/standard_json/yul_no_pvm_codegen.json";

/// The `resolc` YUL mode flag.
pub const RESOLC_YUL_FLAG: &str = "--yul";
/// The `--yul` option was deprecated in Solidity 0.8.27 in favor of `--strict-assembly`.
/// See section `--strict-assembly vs. --yul` in the [release announcement](https://soliditylang.org/blog/2024/09/04/solidity-0.8.27-release-announcement/).
pub const SOLC_YUL_FLAG: &str = "--strict-assembly";

/// Common `resolc` CLI optimization settings.
pub struct ResolcOptSettings;

impl ResolcOptSettings {
    pub const NONE: &'static str = "-O0";
    pub const PERFORMANCE: &'static str = "-O3";
    pub const SIZE: &'static str = "-Oz";
}

/// Common `solc` CLI optimization settings for `--optimize-runs`.
pub struct SolcOptSettings;

impl SolcOptSettings {
    pub const NONE: &'static str = "0";
    pub const PERFORMANCE: &'static str = "20000";
    pub const SIZE: &'static str = "1";
}

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

/// Executes the `resolc` command with the given `arguments`.
pub fn execute_resolc(arguments: &[&str]) -> CommandResult {
    execute_command("resolc", arguments, None)
}

/// Executes the `resolc` command with the given `arguments` and file path passed to `stdin`.
pub fn execute_resolc_with_stdin_input(arguments: &[&str], stdin_file_path: &str) -> CommandResult {
    execute_command("resolc", arguments, Some(stdin_file_path))
}

/// Executes the `solc` command with the given `arguments`.
pub fn execute_solc(arguments: &[&str]) -> CommandResult {
    execute_command(SolcCompiler::DEFAULT_EXECUTABLE_NAME, arguments, None)
}

/// Executes the `solc` command with the given `arguments` and file path passed to `stdin`.
pub fn execute_solc_with_stdin_input(arguments: &[&str], stdin_file_path: &str) -> CommandResult {
    execute_command(
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        arguments,
        Some(stdin_file_path),
    )
}

/// Executes the `command` with the given `arguments` and optional file path passed to `stdin`.
pub fn execute_command(
    command: &str,
    arguments: &[&str],
    stdin_file_path: Option<&str>,
) -> CommandResult {
    log::trace!(
        "executing command: '{command} {}{}'",
        arguments.join(" "),
        stdin_file_path
            .map(|argument| format!("< {argument}"))
            .unwrap_or_default()
    );

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

/// Asserts that the exit codes of executing `solc` and `resolc` are equal.
pub fn assert_equal_exit_codes(solc_result: &CommandResult, resolc_result: &CommandResult) {
    assert_eq!(solc_result.code, resolc_result.code,);
}

/// Asserts that the command terminated successfully with a `0` exit code.
pub fn assert_command_success(result: &CommandResult, error_message_prefix: &str) {
    assert!(
        result.success,
        "{error_message_prefix} should succeed with exit code {}, got {}.\nDetails: {}",
        revive_common::EXIT_CODE_SUCCESS,
        result.code,
        result.stderr
    );
}

/// Asserts that the command terminated with an error and a non-`0` exit code.
pub fn assert_command_failure(result: &CommandResult, error_message_prefix: &str) {
    assert!(
        !result.success,
        "{error_message_prefix} should fail with exit code {}, got {}.",
        revive_common::EXIT_CODE_FAILURE,
        result.code
    );
}

/// Gets the absolute path of a file. The `relative_path` must
/// be relative to the `resolc` crate.
/// Panics if the path does not exist or is not an accessible file.
pub fn absolute_path(relative_path: &str) -> String {
    let absolute_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    if !absolute_path.is_file() {
        panic!("expected a file at `{}`", absolute_path.display());
    }

    absolute_path.to_string_lossy().into_owned()
}
