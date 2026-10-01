//! The tests for running resolc with the revert strings option.

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc, CommandResult, PVM_BLOB_START, RESOLC_YUL_FLAG,
    SOLIDITY_REVERT_STRINGS_CONTRACT_PATH, YUL_CONTRACT_PATH,
};

const REVERT_STRINGS_OPTION: &str = "--revert-strings";
const METADATA_REVERT_STRINGS_KEY: &str = "revertStrings";

/// Compiles the revert strings contract, passing `mode` to `--revert-strings`
/// if given, and any given extra arguments, and returns the command result if successful.
fn compile_with_revert_strings(mode: Option<&str>, extra_arguments: &[&str]) -> CommandResult {
    let mut arguments = vec![SOLIDITY_REVERT_STRINGS_CONTRACT_PATH];
    arguments.extend_from_slice(extra_arguments);
    if let Some(mode) = mode {
        arguments.extend([REVERT_STRINGS_OPTION, mode]);
    }
    let result = execute_resolc(&arguments);
    assert_command_success(&result, "Compiling the revert strings contract");

    result
}

/// Compiles the revert strings contract, passing `mode` to `--revert-strings`
/// if given, and returns the bytecode.
fn compile(mode: Option<&str>) -> String {
    let result = compile_with_revert_strings(mode, &["--bin"]);
    let blob = result
        .stdout
        .split("Binary:\n")
        .nth(1)
        .expect("the output should contain the bytecode")
        .trim()
        .to_owned();
    assert!(
        blob.starts_with(PVM_BLOB_START),
        "expected a binary blob starting with `{PVM_BLOB_START}`",
    );

    blob
}

/// Compiles the revert strings contract to combined JSON, passing `mode` to `--revert-strings`
/// if given, and returns the bytecode and metadata.
fn compile_to_combined_json(mode: Option<&str>) -> (String, String) {
    let result = compile_with_revert_strings(mode, &["--combined-json", "bin,metadata"]);

    let output: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("Combined JSON output should be valid JSON");
    let contract_name = format!("{SOLIDITY_REVERT_STRINGS_CONTRACT_PATH}:C");
    let field = |name: &str| {
        output["contracts"][&contract_name][name]
            .as_str()
            .unwrap_or_else(|| {
                panic!("the contract `{contract_name}` should have the `{name}` field")
            })
            .to_owned()
    };

    let bytecode = field("bin");
    assert!(
        bytecode.starts_with(PVM_BLOB_START),
        "expected a binary blob starting with `{PVM_BLOB_START}`",
    );

    (bytecode, field("metadata"))
}

#[test]
fn compiles_with_valid_mode() {
    let bytecode = compile(None);
    assert_eq!(
        compile(Some("default")),
        bytecode,
        "the default revert strings mode should build the same bytecode as omitting the option"
    );
    assert!(
        compile(Some("strip")).len() < bytecode.len(),
        "the strip revert strings mode should yield smaller bytecode than the default"
    );
    assert!(
        compile(Some("debug")).len() > bytecode.len(),
        "the debug revert strings mode should yield larger bytecode than the default"
    );
}

#[test]
fn compiles_to_combined_json_with_valid_mode() {
    let (bytecode, metadata) = compile_to_combined_json(None);
    let (bytecode_strip, metadata_strip) = compile_to_combined_json(Some("strip"));
    assert!(
        bytecode_strip.len() < bytecode.len(),
        "the strip revert strings mode should yield smaller combined JSON bytecode"
    );
    assert!(
        !metadata.contains(METADATA_REVERT_STRINGS_KEY),
        "the combined JSON metadata should not record a revert strings mode by default"
    );
    assert!(
        metadata_strip.contains(METADATA_REVERT_STRINGS_KEY),
        "the combined JSON metadata should record the strip revert strings mode"
    );
}

#[test]
fn invalid_value() {
    let arguments = &[
        SOLIDITY_REVERT_STRINGS_CONTRACT_PATH,
        "--bin",
        REVERT_STRINGS_OPTION,
        "invalid",
    ];
    let resolc_result = execute_resolc(arguments);
    assert_command_failure(
        &resolc_result,
        "Providing an unsupported revert strings mode",
    );
    assert!(resolc_result
        .stderr
        .contains("unknown revert strings mode: `invalid`"));

    let solc_result = execute_solc(arguments);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn invalid_extra_flags() {
    for flag in [RESOLC_YUL_FLAG, "--link"] {
        let result = execute_resolc(&[YUL_CONTRACT_PATH, flag, REVERT_STRINGS_OPTION, "strip"]);
        assert_command_failure(&result, "Providing the revert strings option");
        assert!(
            result
                .stderr
                .contains("`revert-strings` is not used in Yul and linker modes."),
            "the `{flag}` mode should reject the revert strings option"
        );
    }
}
