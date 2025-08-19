//! The tests for running resolc with output directory option.

#![cfg(test)]

use std::path::Path;

use rstest::rstest;

use crate::tests::cli::utils;

const OUTPUT_DIRECTORY: &str = "src/tests/cli/artifacts";
const OUTPUT_BIN_FILE_PATH: &str = "src/tests/cli/artifacts/contract.sol:C.pvm";
const OUTPUT_ASM_FILE_PATH: &str = "src/tests/cli/artifacts/contract.sol:C.pvmasm";
const OUTPUT_LLVM_OPTIMIZED_FILE_PATH: &str =
    "src/tests/cli/artifacts/src_tests_cli_contracts_solidity_contract.sol.C.optimized.ll";
const OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH: &str =
    "src/tests/cli/artifacts/src_tests_cli_contracts_solidity_contract.sol.C.unoptimized.ll";

fn file_exists(path: &str) -> bool {
    Path::new(path).try_exists().unwrap()
}

fn file_is_empty(path: &str) -> bool {
    Path::new(path).metadata().unwrap().len() == 0
}

fn assert_valid_output_file(
    result: &utils::CommandResult,
    output_file_type: &str,
    output_file_path: &str,
) {
    utils::assert_command_success(result, "Providing an output directory");

    assert!(
        result.output.contains("Compiler run successful"),
        "Expected the compiler output to contain a success message.",
    );

    assert!(
        file_exists(output_file_path),
        "Expected the {output_file_type} output file `{output_file_path}` to exist."
    );

    assert!(
        !file_is_empty(output_file_path),
        "Expected the {output_file_type} output file `{output_file_path}` to not be empty."
    );
}

#[rstest]
#[case::binary_output("--bin", OUTPUT_BIN_FILE_PATH)]
#[case::assembly_output("--asm", OUTPUT_ASM_FILE_PATH)]
fn writes_to_file(#[case] output_file_type: &str, #[case] output_file_path: &str) {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "--overwrite",
        "-O3",
        output_file_type,
        "--output-dir",
        OUTPUT_DIRECTORY,
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, output_file_type, output_file_path);
}

#[rstest]
#[case::binary_output("--bin", OUTPUT_BIN_FILE_PATH)]
#[case::assembly_output("--asm", OUTPUT_ASM_FILE_PATH)]
fn writes_debug_info_to_file_unoptimized(
    #[case] output_file_type: &str,
    #[case] output_file_path: &str,
) {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        output_file_type,
        "--output-dir",
        OUTPUT_DIRECTORY,
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, output_file_type, output_file_path);
}

#[rstest]
#[case::binary_output("--bin", OUTPUT_BIN_FILE_PATH)]
#[case::assembly_output("--asm", OUTPUT_ASM_FILE_PATH)]
fn writes_debug_info_to_file_optimized(
    #[case] output_file_type: &str,
    #[case] output_file_path: &str,
) {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        output_file_type,
        "--output-dir",
        OUTPUT_DIRECTORY,
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, output_file_type, output_file_path);
}

#[test]
fn writes_llvm_debug_info_to_file_unoptimized() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--disable-solc-optimizer",
        "--overwrite",
        "--debug-output-dir",
        OUTPUT_DIRECTORY,
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, "llvm", OUTPUT_LLVM_UNOPTIMIZED_FILE_PATH);
}

#[test]
fn writes_llvm_debug_info_to_file_optimized() {
    let arguments = &[
        utils::SOLIDITY_CONTRACT_PATH,
        "-g",
        "--overwrite",
        "--debug-output-dir",
        OUTPUT_DIRECTORY,
    ];
    let result = utils::execute_resolc(arguments);
    assert_valid_output_file(&result, "llvm", OUTPUT_LLVM_OPTIMIZED_FILE_PATH);
}
