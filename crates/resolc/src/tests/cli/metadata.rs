//! The tests for the contract metadata emitted by `resolc`.

use revive_solc_json_interface::{PolkaVMDefaultHeapMemorySize, PolkaVMDefaultStackMemorySize};

use crate::cli_utils::{
    absolute_path, assert_command_success, execute_resolc, SOLIDITY_HEAP_STACK_SIZE_PATH,
};

/// Compiles the heap and stack size contract with the given arguments and returns its metadata.
fn compile_metadata(arguments: &[&str]) -> String {
    let path = absolute_path(SOLIDITY_HEAP_STACK_SIZE_PATH);
    let mut all_arguments = vec![path.as_str(), "--metadata"];
    all_arguments.extend_from_slice(arguments);
    let result = execute_resolc(&all_arguments);
    assert_command_success(&result, "Compiling the heap and stack size contract");
    result
        .stdout
        .split("Metadata:\n")
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
