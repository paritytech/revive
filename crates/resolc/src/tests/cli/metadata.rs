//! The tests for the contract metadata emitted by `resolc`.

use polkavm_common::program::ProgramBlob;
use revive_common::Keccak256;
use revive_solc_json_interface::{PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize};

use crate::cli_utils::{
    absolute_path, assert_command_success, execute_resolc, CommandResult,
    SOLIDITY_HEAP_STACK_SIZE_PATH,
};

/// The `stdout` message immediately preceding the binary blob.
const BINARY_PREFIX: &str = "Binary:\n";

/// The `stdout` message immediately preceding the metadata.
const METADATA_PREFIX: &str = "Metadata:\n";

/// Returns the first line following `prefix` in the `stdout` of `result`.
fn output_after(result: &CommandResult, prefix: &str) -> String {
    result
        .stdout
        .split(prefix)
        .nth(1)
        .and_then(|output| output.lines().next())
        .unwrap_or_else(|| panic!("the output should contain {prefix:?}"))
        .trim()
        .to_owned()
}

/// Returns the metadata hash embedded in the hex encoded PVM `blob`.
pub fn blob_metadata_hash(blob: &str) -> Vec<u8> {
    let blob = hex::decode(blob).expect("the blob should be hex encoded");
    ProgramBlob::parse(blob.into())
        .expect("the blob should be a valid PVM blob")
        .metadata_hash()
        .to_vec()
}

/// Compiles the heap and stack size contract with the given arguments and returns its metadata.
fn compile_metadata(arguments: &[&str]) -> String {
    let path = absolute_path(SOLIDITY_HEAP_STACK_SIZE_PATH);
    let mut all_arguments = vec![path.as_str(), "--metadata"];
    all_arguments.extend_from_slice(arguments);
    let result = execute_resolc(&all_arguments);
    assert_command_success(&result, "Compiling the heap and stack size contract");
    result
        .stdout
        .split(METADATA_PREFIX)
        .nth(1)
        .expect("the output should contain the metadata")
        .trim()
        .to_owned()
}

/// The metadata records the memory configuration, the pipeline and debug information.
#[test]
fn metadata_records_code_settings() {
    let default_metadata = compile_metadata(&[]);
    for arguments in [
        ["--heap-size", "65536"].as_slice(),
        ["--stack-size", "65536"].as_slice(),
        ["--newyork"].as_slice(),
        ["-g"].as_slice(),
    ] {
        assert_ne!(
            compile_metadata(arguments),
            default_metadata,
            "the metadata should differ from the default build with {arguments:?}"
        );
    }

    let heap_size = PolkaVMDefaultHeapMemorySize.to_string();
    let stack_size = PolkaVMDefaultStackMemorySize.to_string();
    assert_eq!(
        compile_metadata(&["--heap-size", &heap_size, "--stack-size", &stack_size]),
        default_metadata,
        "the metadata should not differ when passing the default heap and stack size"
    );
}

/// The blob embeds the keccak256 hash of the contract metadata by default.
#[test]
fn bin_embeds_keccak256_metadata_hash() {
    let path = absolute_path(SOLIDITY_HEAP_STACK_SIZE_PATH);
    let result = execute_resolc(&[path.as_str(), "--bin", "--metadata"]);
    assert_command_success(&result, "Compiling the heap and stack size contract");

    let metadata = output_after(&result, METADATA_PREFIX);
    assert_eq!(
        blob_metadata_hash(&output_after(&result, BINARY_PREFIX)),
        Keccak256::from_slice(metadata.as_bytes()).as_bytes(),
        "the blob should embed the keccak256 hash of the metadata"
    );
}

/// The blob embeds no metadata hash with `--metadata-hash none`.
#[test]
fn bin_omits_metadata_hash_with_none() {
    let path = absolute_path(SOLIDITY_HEAP_STACK_SIZE_PATH);
    let result = execute_resolc(&[path.as_str(), "--bin", "--metadata-hash", "none"]);
    assert_command_success(&result, "Compiling the heap and stack size contract");

    assert!(
        blob_metadata_hash(&output_after(&result, BINARY_PREFIX)).is_empty(),
        "the blob should not embed a metadata hash"
    );
}
