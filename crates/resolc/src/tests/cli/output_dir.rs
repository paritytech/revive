//! The tests for running resolc with output directory option.

use std::path::Path;

use tempfile::tempdir;

use crate::cli_utils::{
    assert_command_success, execute_resolc, CommandResult, SOLIDITY_COLLISION_NESTED_PATH,
    SOLIDITY_COLLISION_UNDERSCORE_PATH, SOLIDITY_CONTRACT_PATH,
};

const OUTPUT_BIN_FILE_PATH: &str = "contract.sol:C.pvm";
const OUTPUT_ASM_FILE_PATH: &str = "contract.sol:C.pvmasm";
const OUTPUT_LLVM_OPTIMIZED_FILE_PATH: &str = "src_tests_data_solidity_contract.sol.C.optimized.ll";
const OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH: &str =
    "src_tests_data_solidity_contract.sol.C.unoptimized.ll";
const OUTPUT_YUL_COLLISION_NESTED_FILE_PATH: &str =
    "src_tests_data_solidity_collision_a_c.sol.C.yul";
const OUTPUT_YUL_COLLISION_UNDERSCORE_FILE_PATH: &str =
    "src_tests_data_solidity_collision_a%5Fc.sol.C.yul";

fn assert_valid_output_file(
    result: &CommandResult,
    debug_output_directory: &Path,
    output_file_name: &str,
) {
    assert_command_success(result, "Providing an output directory");

    assert!(result.stderr.contains("Compiler run successful"),);

    let file = debug_output_directory.to_path_buf().join(output_file_name);

    assert!(file.exists(), "Artifact should exist: {}", file.display());

    assert_ne!(
        file.metadata().unwrap().len(),
        0,
        "Artifact shouldn't be empty: {}",
        file.display()
    );
}

#[test]
fn writes_to_file() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        "--overwrite",
        "-O3",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_debug_info_to_file_unoptimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_debug_info_to_file_optimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_llvm_debug_info_to_file_unoptimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        "--debug-output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH);
}

#[test]
fn writes_llvm_debug_info_to_file_optimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        "--debug-output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_LLVM_OPTIMIZED_FILE_PATH);
}

/// Contracts whose paths only differ by `/` versus `_` get separate debug artifacts.
#[test]
fn debug_output_file_names_do_not_collide() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        "--bin",
        "--debug-output-dir",
        temp_dir.path().to_str().unwrap(),
        SOLIDITY_COLLISION_NESTED_PATH,
        SOLIDITY_COLLISION_UNDERSCORE_PATH,
    ];
    let result = execute_resolc(arguments);
    assert_command_success(&result, "Providing a debug output directory");

    for (file_name, source_path) in [
        (
            OUTPUT_YUL_COLLISION_NESTED_FILE_PATH,
            SOLIDITY_COLLISION_NESTED_PATH,
        ),
        (
            OUTPUT_YUL_COLLISION_UNDERSCORE_FILE_PATH,
            SOLIDITY_COLLISION_UNDERSCORE_PATH,
        ),
    ] {
        let yul = std::fs::read_to_string(temp_dir.path().join(file_name)).unwrap();
        assert!(
            yul.contains(&format!("\"{source_path}\"")),
            "{file_name} should be generated from {source_path}"
        );
    }
}
