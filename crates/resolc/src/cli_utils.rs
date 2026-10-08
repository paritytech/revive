//! Common utilities used for CLI tests.

use std::{
    fs::File,
    path::{Path, PathBuf},
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
/// The reproducer from paritytech/revive#628: a library and a contract using its address.
pub const SOLIDITY_LIBRARY_ADDRESS_CONTRACT_PATH: &str =
    "src/tests/data/solidity/use_library_address.sol";
/// The verbatim reproducer from paritytech/revive#560: a folded guard plus an
/// inlined for-loop that crashed the newyork pipeline under `--disable-solc-optimizer`.
pub const SOLIDITY_FOLDED_GUARD_INLINED_LOOP_PATH: &str =
    "src/tests/data/solidity/folded_guard_inlined_loop.sol";
/// The Solidity contract importing the simple contract through a remapping.
pub const SOLIDITY_REMAPPED_IMPORT_PATH: &str = "src/tests/data/solidity/remapped_import.sol";
/// The Solidity contract importing the simple contract through an include path.
pub const SOLIDITY_INCLUDED_IMPORT_PATH: &str = "src/tests/data/solidity/included_import.sol";
/// The Solidity contract importing `target/Target.sol`, to be remapped to one of two directories.
pub const SOLIDITY_REMAPPINGS_ORDER_MAIN_PATH: &str =
    "src/tests/data/solidity/remappings_order/main.sol";
/// The directory with the `Target` contract defining `first`.
pub const SOLIDITY_REMAPPINGS_ORDER_FIRST_DIRECTORY: &str =
    "src/tests/data/solidity/remappings_order/b_first/";
/// The directory with the `Target` contract defining `second`.
pub const SOLIDITY_REMAPPINGS_ORDER_SECOND_DIRECTORY: &str =
    "src/tests/data/solidity/remappings_order/a_second/";
/// The reproducer from paritytech/revive#624 with 129 immutables, one more than the limit allows.
pub const SOLIDITY_IMMUTABLES_OVER_LIMIT_PATH: &str =
    "src/tests/data/solidity/immutables_over_limit.sol";
/// The reproducer from paritytech/revive#624 with 128 immutables, exactly at the limit.
pub const SOLIDITY_IMMUTABLES_AT_LIMIT_PATH: &str =
    "src/tests/data/solidity/immutables_at_limit.sol";
/// A contract using `blobhash` and `blobbasefee`
pub const SOLIDITY_BLOB_OPCODES_PATH: &str = "src/tests/data/solidity/blob_opcodes.sol";
/// The verbatim reproducer from paritytech/revive#625: a contract calling an external library.
pub const SOLIDITY_LIBRARY_CALL_CONTRACT_PATH: &str = "src/tests/data/solidity/library_call.sol";
/// The paritytech/revive#622 reproducer contract at `a/c.sol`.
pub const SOLIDITY_COLLISION_NESTED_PATH: &str = "src/tests/data/solidity/collision/a/c.sol";
/// The paritytech/revive#622 reproducer contract at `a_c.sol`.
pub const SOLIDITY_COLLISION_UNDERSCORE_PATH: &str = "src/tests/data/solidity/collision/a_c.sol";
/// Two contracts named `First` and `Second` returning `1` and `2`.
pub const SOLIDITY_TWO_CONTRACTS_PATH: &str = "src/tests/data/solidity/two_contracts.sol";
/// The verbatim reproducer from paritytech/revive#632: two contracts calling the same library.
pub const SOLIDITY_LINK_INDEPENDENT_OBJECTS_PATH: &str =
    "src/tests/data/solidity/link_independent_objects.sol";
/// A Solidity contract whose `require` has a reason string.
pub const SOLIDITY_REVERT_STRINGS_CONTRACT_PATH: &str =
    "src/tests/data/solidity/revert_strings.sol";

/// The simple YUL contract test fixture path.
pub const YUL_CONTRACT_PATH: &str = "src/tests/data/yul/contract.yul";
/// The memeset YUL contract test fixture path.
pub const YUL_MEMSET_CONTRACT_PATH: &str = "src/tests/data/yul/memset.yul";
/// The return YUL contract test fixture path.
pub const YUL_RETURN_CONTRACT_PATH: &str = "src/tests/data/yul/return.yul";
/// The invalid YUL contract test fixture path (hex literal with odd number of nibbles).
pub const YUL_INVALID_HEX_NIBBLES_PATH: &str = "src/tests/data/yul/invalid_hex_nibbles.yul";
/// Yul contract with duplicate function names in switch cases.
pub const YUL_DUPLICATE_FUNCTIONS_SWITCH_PATH: &str =
    "src/tests/data/yul/duplicate_functions_switch.yul";
/// Yul contract with duplicate function names in deeply nested switch cases.
pub const YUL_DUPLICATE_FUNCTIONS_DEEP_NESTING_PATH: &str =
    "src/tests/data/yul/duplicate_functions_deep_nesting.yul";
/// Yul contract whose `_deployed` runtime object has an empty code block.
pub const YUL_EMPTY_RUNTIME_OBJECT_PATH: &str = "src/tests/data/yul/empty_runtime_object.yul";
/// Yul contract carrying deploy code only, without the `_deployed` runtime sub-object.
pub const YUL_DEPLOY_ONLY_OBJECT_PATH: &str = "src/tests/data/yul/deploy_only_object.yul";
/// Yul contract with a sibling object that is not a contract of its own.
pub const YUL_SIBLING_OBJECTS_PATH: &str = "src/tests/data/yul/sibling_objects.yul";
/// Yul contract addressing a nested object through the dotted notation.
pub const YUL_DOTTED_OBJECT_PATH: &str = "src/tests/data/yul/dotted_object_path.yul";

/// The standard JSON contracts test fixture path.
pub const STANDARD_JSON_CONTRACTS_PATH: &str =
    "src/tests/data/standard_json/solidity_contracts.json";
/// The standard JSON fixture with the 129 immutables reproducer from paritytech/revive#624.
pub const STANDARD_JSON_IMMUTABLES_OVER_LIMIT_PATH: &str =
    "src/tests/data/standard_json/immutables_over_limit.json";
/// The standard JSON contracts test fixture path that requests every single output.
pub const STANDARD_JSON_ALL_OUTPUTS_PATH: &str = "src/tests/data/standard_json/all_outputs.json";
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
/// The verbatim reproducer from paritytech/revive#627: a library address used only in a switch expression.
pub const STANDARD_JSON_SWITCH_MISSING_LIBRARIES_PATH: &str =
    "src/tests/data/standard_json/switch_missing_libraries.json";
/// The standard JSON fixture remapping `target/` to `b_first/` and then to `a_second/`.
pub const STANDARD_JSON_REMAPPINGS_ORDER_PATH: &str =
    "src/tests/data/standard_json/remappings_order.json";
/// The standard JSON PVM codegen all wildcard test fixture path.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_PVM_CODEGEN_ALL_WILDCARD_PATH: &str =
    "src/tests/data/standard_json/pvm_codegen_all_wildcard.json";
/// A standard JSON fixture enabling the newyork pipeline via `settings.polkavm.newyork`.
///
/// Shares the same source as [`STANDARD_JSON_NEWYORK_DISABLED_PATH`] so the two
/// can be compared to confirm the input field selects a different pipeline.
pub const STANDARD_JSON_NEWYORK_ENABLED_PATH: &str =
    "src/tests/data/standard_json/newyork_enabled.json";
/// The same source as [`STANDARD_JSON_NEWYORK_ENABLED_PATH`] without the
/// `settings.polkavm.newyork` field, so it compiles through the stock pipeline.
pub const STANDARD_JSON_NEWYORK_DISABLED_PATH: &str =
    "src/tests/data/standard_json/newyork_disabled.json";
/// A `"language": "Yul"` standard JSON fixture enabling newyork via `settings.polkavm.newyork`.
pub const STANDARD_JSON_YUL_NEWYORK_ENABLED_PATH: &str =
    "src/tests/data/standard_json/yul_newyork_enabled.json";
/// The same Yul source as [`STANDARD_JSON_YUL_NEWYORK_ENABLED_PATH`] without the
/// `settings.polkavm.newyork` field, so it compiles through the stock pipeline.
pub const STANDARD_JSON_YUL_NEWYORK_DISABLED_PATH: &str =
    "src/tests/data/standard_json/yul_newyork_disabled.json";
/// A contract creating a contract that calls a library, compiled normally.
pub const STANDARD_JSON_FACTORY_DEPENDENCY_LIBRARIES_PATH: &str =
    "src/tests/data/standard_json/factory_dependency_libraries.json";
/// The same input as [`STANDARD_JSON_FACTORY_DEPENDENCY_LIBRARIES_PATH`] with `detectMissingLibraries` set.
pub const STANDARD_JSON_FACTORY_DEPENDENCY_LIBRARIES_DETECT_PATH: &str =
    "src/tests/data/standard_json/factory_dependency_libraries_detect.json";
/// The standard JSON PVM codegen for all files on a per-file basis test fixture path.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH: &str =
    "src/tests/data/standard_json/pvm_codegen_per_file.json";
/// The standard JSON PVM codegen for one out of many files test fixture path.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_PVM_CODEGEN_ONE_FILE_PATH: &str =
    "src/tests/data/standard_json/pvm_codegen_one_file.json";
/// The standard JSON no PVM codegen for any file on a per-file basis test fixture path.
///
/// This omits `evm` bytecode selection flags, which should thereby
/// prevent PVM bytecode generation.
///
/// These contracts are similar to ones used in an example project.
pub const STANDARD_JSON_NO_PVM_CODEGEN_PER_FILE_PATH: &str =
    "src/tests/data/standard_json/no_pvm_codegen_per_file.json";
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
/// The standard JSON input from paritytech/revive#623 with a heap size outside the PVM memory map.
pub const STANDARD_JSON_HEAP_SIZE_OUT_OF_RANGE_PATH: &str =
    "src/tests/data/standard_json/heap_size_out_of_range.json";
/// A standard JSON input with a heap size the polkavm linker fails to link.
pub const STANDARD_JSON_HEAP_SIZE_LINKER_FAILURE_PATH: &str =
    "src/tests/data/standard_json/heap_size_linker_failure.json";
/// A standard JSON fixture with `settings.debug.revertStrings` set to `"default"`,
/// so solc keeps the `require` reason string of its contract.
pub const STANDARD_JSON_REVERT_STRINGS_DEFAULT_PATH: &str =
    "src/tests/data/standard_json/revert_strings_default.json";
/// A standard JSON fixture with `settings.debug.revertStrings` set to `"strip"`,
/// so solc removes the reason string.
pub const STANDARD_JSON_REVERT_STRINGS_STRIP_PATH: &str =
    "src/tests/data/standard_json/revert_strings_strip.json";
/// A standard JSON fixture with `settings.debug.revertStrings` set to `"verboseDebug"`.
pub const STANDARD_JSON_REVERT_STRINGS_VERBOSE_DEBUG_PATH: &str =
    "src/tests/data/standard_json/revert_strings_verbose_debug.json";
/// A `"language": "Yul"` standard JSON fixture with `settings.debug.revertStrings` set to `"strip"`,
/// which solc rejects for Yul.
pub const STANDARD_JSON_YUL_REVERT_STRINGS_STRIP_PATH: &str =
    "src/tests/data/standard_json/yul_revert_strings_strip.json";
/// A standard JSON fixture without `settings.debug`, so solc adds its default
/// source location annotations to the Yul IR.
pub const STANDARD_JSON_DEBUG_INFO_DEFAULT_PATH: &str =
    "src/tests/data/standard_json/debug_info_default.json";
/// A standard JSON fixture with an empty `settings.debug.debugInfo`, so solc omits its
/// source location annotations from the Yul IR.
pub const STANDARD_JSON_DEBUG_INFO_EMPTY_PATH: &str =
    "src/tests/data/standard_json/debug_info_empty.json";
/// A standard JSON fixture requesting codegen for all contracts in all files
/// via the "all" (`*`) wildcard, but only additional fields for specific files.
pub const STANDARD_JSON_MIX_ALL_WILDCARD_AND_FILE_SELECTION_PATH: &str =
    "src/tests/data/standard_json/mix_all_wildcard_and_file_selection.json";

/// The `resolc` YUL mode flag.
pub const RESOLC_YUL_FLAG: &str = "--yul";
/// The `--yul` option was deprecated in Solidity 0.8.27 in favor of `--strict-assembly`.
/// See section `--strict-assembly vs. --yul` in the [release announcement](https://soliditylang.org/blog/2024/09/04/solidity-0.8.27-release-announcement/).
pub const SOLC_YUL_FLAG: &str = "--strict-assembly";

/// The starting hex value of a PVM blob (encoding of `"PVM"`).
pub const PVM_BLOB_START: &str = "50564d";

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

/// Executes the `resolc` command with the given `arguments` inside `directory`.
pub fn execute_resolc_in_directory(arguments: &[&str], directory: &Path) -> CommandResult {
    execute_command_in_directory("resolc", arguments, None, Some(directory))
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
    execute_command_in_directory(command, arguments, stdin_file_path, None)
}

/// Executes the `command` like [`execute_command`], inside `directory` if given.
pub fn execute_command_in_directory(
    command: &str,
    arguments: &[&str],
    stdin_file_path: Option<&str>,
    directory: Option<&Path>,
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
    let mut command = Command::new(command);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let result = command
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
