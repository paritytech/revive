//! The tests for running resolc with output directory option.

use std::path::{Component, Path, PathBuf};

use tempfile::tempdir;

use crate::cli_utils::{
    assert_command_failure, assert_command_success, execute_resolc, execute_resolc_in_directory,
    CommandResult, SOLIDITY_COLLISION_NESTED_PATH, SOLIDITY_COLLISION_UNDERSCORE_PATH,
    SOLIDITY_CONTRACT_PATH, SOLIDITY_DUPLICATE_FIRST_PATH, SOLIDITY_DUPLICATE_SECOND_PATH,
};

const OUTPUT_BIN_FILE_PATH: &str = "src/tests/data/solidity/contract.sol:C.pvm";
const OUTPUT_ASM_FILE_PATH: &str = "src/tests/data/solidity/contract.sol:C.pvmasm";
const OUTPUT_DUPLICATE_FIRST_FILE_PATH: &str =
    "src/tests/data/solidity/first/Duplicate.sol:Duplicate.pvm";
const OUTPUT_DUPLICATE_SECOND_FILE_PATH: &str =
    "src/tests/data/solidity/second/Duplicate.sol:Duplicate.pvm";
const OUTPUT_DUPLICATE_FIRST_METADATA_FILE_PATH: &str =
    "src/tests/data/solidity/first/Duplicate.sol:Duplicate.json";
const OUTPUT_DUPLICATE_SECOND_METADATA_FILE_PATH: &str =
    "src/tests/data/solidity/second/Duplicate.sol:Duplicate.json";
const OUTPUT_DUPLICATE_FILE_NAME: &str = "Duplicate.sol:Duplicate.pvm";
const OUTPUT_DUPLICATE_PARENT_FILE_PATH: &str = "%2E%2E/contracts/Duplicate.sol:Duplicate.pvm";
const OUTPUT_DUPLICATE_NESTED_FILE_PATH: &str = "contracts/Duplicate.sol:Duplicate.pvm";
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

/// Contracts with the same name in files with the same name get separate output files.
#[test]
fn output_file_names_do_not_collide() {
    let temp_dir = tempdir().unwrap();
    let arguments = &[
        "--bin",
        "--metadata",
        "-o",
        temp_dir.path().to_str().unwrap(),
        SOLIDITY_DUPLICATE_FIRST_PATH,
        SOLIDITY_DUPLICATE_SECOND_PATH,
    ];
    let result = execute_resolc(arguments);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_DUPLICATE_FIRST_FILE_PATH);
    assert_valid_output_file(&result, temp_dir.path(), OUTPUT_DUPLICATE_SECOND_FILE_PATH);

    let first = std::fs::read(temp_dir.path().join(OUTPUT_DUPLICATE_FIRST_FILE_PATH)).unwrap();
    let second = std::fs::read(temp_dir.path().join(OUTPUT_DUPLICATE_SECOND_FILE_PATH)).unwrap();
    assert_ne!(first, second, "Both contracts should keep their own output");

    for (file_name, source_path) in [
        (
            OUTPUT_DUPLICATE_FIRST_METADATA_FILE_PATH,
            SOLIDITY_DUPLICATE_FIRST_PATH,
        ),
        (
            OUTPUT_DUPLICATE_SECOND_METADATA_FILE_PATH,
            SOLIDITY_DUPLICATE_SECOND_PATH,
        ),
    ] {
        let metadata = std::fs::read_to_string(temp_dir.path().join(file_name)).unwrap();
        assert!(
            metadata.contains(source_path),
            "{file_name} should be generated from {source_path}"
        );
    }
}

/// An absolute source path mirrors its directories inside the output directory, and nothing is
/// written next to the source.
#[test]
fn absolute_source_path_output_stays_inside_output_directory() {
    let temp_dir = tempdir().unwrap();
    let source_directory = temp_dir.path().join("sources");
    let output_directory = temp_dir.path().join("output");
    std::fs::create_dir(&source_directory).unwrap();
    let source_path = source_directory.join("Duplicate.sol");
    std::fs::copy(SOLIDITY_DUPLICATE_FIRST_PATH, &source_path).unwrap();

    let result = execute_resolc(&[
        "--bin",
        "-o",
        output_directory.to_str().unwrap(),
        source_path.to_str().unwrap(),
    ]);
    let mirrored_source_directory = source_directory
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .collect::<PathBuf>();
    assert_valid_output_file(
        &result,
        &output_directory.join(mirrored_source_directory),
        OUTPUT_DUPLICATE_FILE_NAME,
    );
    assert_eq!(
        std::fs::read_dir(&source_directory).unwrap().count(),
        1,
        "Nothing should be written next to the source"
    );
}

/// A source reached through `..` does not share its output files with a source at the same path
/// below the working directory, and both stay inside the output directory.
#[test]
fn parent_directory_output_file_names_do_not_collide() {
    let temp_dir = tempdir().unwrap();
    let working_directory = temp_dir.path().join("work");
    for (source_path, directory) in [
        (SOLIDITY_DUPLICATE_FIRST_PATH, temp_dir.path()),
        (SOLIDITY_DUPLICATE_SECOND_PATH, working_directory.as_path()),
    ] {
        let contracts_directory = directory.join("contracts");
        std::fs::create_dir_all(&contracts_directory).unwrap();
        std::fs::copy(source_path, contracts_directory.join("Duplicate.sol")).unwrap();
    }
    let arguments = &[
        "--bin",
        "-o",
        "out",
        "../contracts/Duplicate.sol",
        "./contracts/Duplicate.sol",
    ];
    let result = execute_resolc_in_directory(arguments, &working_directory);
    let output_directory = working_directory.join("out");
    for file_name in [
        OUTPUT_DUPLICATE_PARENT_FILE_PATH,
        OUTPUT_DUPLICATE_NESTED_FILE_PATH,
    ] {
        assert_valid_output_file(&result, &output_directory, file_name);
    }

    let parent = std::fs::read(output_directory.join(OUTPUT_DUPLICATE_PARENT_FILE_PATH)).unwrap();
    let nested = std::fs::read(output_directory.join(OUTPUT_DUPLICATE_NESTED_FILE_PATH)).unwrap();
    assert_ne!(
        parent, nested,
        "Both contracts should keep their own output"
    );
}

/// A file in the place of an output directory fails the build with an error naming the directory,
/// as the operating system's error alone does not say which path is in the way.
#[test]
fn output_directory_creation_error_names_directory() {
    let temp_dir = tempdir().unwrap();
    let source_directory = Path::new("contracts");
    let output_directory = Path::new("out");
    let source_path = source_directory.join("Duplicate.sol");
    let blocked_directory = output_directory.join(source_directory);
    std::fs::create_dir(temp_dir.path().join(source_directory)).unwrap();
    std::fs::copy(
        SOLIDITY_DUPLICATE_FIRST_PATH,
        temp_dir.path().join(&source_path),
    )
    .unwrap();
    std::fs::create_dir(temp_dir.path().join(output_directory)).unwrap();
    std::fs::File::create(temp_dir.path().join(&blocked_directory)).unwrap();

    let arguments = &[
        "--bin",
        "-o",
        output_directory.to_str().unwrap(),
        source_path.to_str().unwrap(),
    ];
    let result = execute_resolc_in_directory(arguments, temp_dir.path());
    assert_command_failure(&result, "Creating an output directory where a file exists");
    assert!(
        result.stderr.contains(&format!("{blocked_directory:?}")),
        "The error should name {blocked_directory:?}: {}",
        result.stderr
    );
}
