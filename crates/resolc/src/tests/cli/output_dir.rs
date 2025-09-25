//! The tests for running resolc with output directory option.

#![cfg(test)]

use std::path::Path;

use tempfile::tempdir;

use crate::tests::cli::utils;

const OUTPUT_BIN_FILE_PATH: &str = "contract.sol.pvm";
const OUTPUT_ASM_FILE_PATH: &str = "contract.sol.pvmasm";
const OUTPUT_LLVM_OPTIMIZED_FILE_PATH: &str = "src_tests_data_solidity_contract.sol.C.optimized.ll";
const OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH: &str =
    "src_tests_data_solidity_contract.sol.C.unoptimized.ll";

fn assert_valid_output_file(
    result: &utils::CommandResult,
    debug_output_directory: &Path,
    output_file_name: &str,
) {
    utils::assert_command_success(result, "Providing an output directory");

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
        utils::SOLIDITY_CONTRACT_PATH,
        "--overwrite",
        "-O3",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_debug_info_to_file_unoptimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_debug_info_to_file_optimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        "--bin",
        "--asm",
        "--output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_BIN_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_ASM_FILE_PATH);
}

#[test]
fn writes_llvm_debug_info_to_file_unoptimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        "--debug-output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH);
}

#[test]
fn writes_llvm_debug_info_to_file_optimized() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        "--debug-output-dir",
        temp_dir.path().to_str().unwrap(),
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_LLVM_OPTIMIZED_FILE_PATH);
}
