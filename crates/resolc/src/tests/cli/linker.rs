use crate::cli_utils::{
    assert_command_failure, assert_command_success, execute_resolc,
    SOLIDITY_DEPENDENCY_CONTRACT_PATH, SOLIDITY_LIBRARY_CALL_CONTRACT_PATH,
    SOLIDITY_LINK_INDEPENDENT_OBJECTS_PATH,
};

/// Test deploy time linking a contract with unresolved factory dependencies.
#[test]
fn deploy_time_linking_works() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let output_directory = temp_dir.path().to_path_buf();
    let source_path = temp_dir.path().to_path_buf().join("dependency.sol");
    std::fs::copy(SOLIDITY_DEPENDENCY_CONTRACT_PATH, &source_path).unwrap();

    assert_command_success(
        &execute_resolc(&[
            source_path.to_str().unwrap(),
            "--bin",
            "-o",
            &output_directory.to_string_lossy(),
        ]),
        "Missing libraries should compile fine",
    );

    let dependency_blob_path = temp_dir
        .path()
        .to_path_buf()
        .join("dependency.sol:Dependency.pvm");
    let blob_path = temp_dir
        .path()
        .to_path_buf()
        .join("dependency.sol:TestAssert.pvm");

    let output = execute_resolc(&[
        "--link",
        blob_path.to_str().unwrap(),
        dependency_blob_path.to_str().unwrap(),
    ]);
    assert_command_success(&output, "The linker mode with missing library should work");
    assert!(output.stdout.contains("still unresolved"));

    let assert_library_path = format!(
        "{}:Assert=0x0000000000000000000000000000000000000001",
        source_path.to_str().unwrap()
    );
    let assert_ne_library_path = format!(
        "{}:AssertNe=0x0000000000000000000000000000000000000002",
        source_path.to_str().unwrap()
    );
    let output = execute_resolc(&[
        "--link",
        "--libraries",
        &assert_library_path,
        "--libraries",
        &assert_ne_library_path,
        blob_path.to_str().unwrap(),
        dependency_blob_path.to_str().unwrap(),
    ]);
    assert_command_success(&output, "The linker mode with all library should work");
    assert!(!output.stdout.contains("still unresolved"));
}

#[test]
fn emits_unlinked_binary_warning() {
    let output = execute_resolc(&[SOLIDITY_DEPENDENCY_CONTRACT_PATH, "--bin"]);
    assert_command_success(&output, "Missing libraries should compile fine");
    assert!(output.stderr.contains("is unlinked"));
}

/// Test that a library argument with more than one `=` is rejected.
#[test]
fn libraries_multiple_equal_signs_fails() {
    let library = format!(
        "{SOLIDITY_LIBRARY_CALL_CONTRACT_PATH}:L=0x1111111111111111111111111111111111111111=0x2222222222222222222222222222222222222222"
    );
    let output = execute_resolc(&[
        "--bin",
        "--libraries",
        &library,
        SOLIDITY_LIBRARY_CALL_CONTRACT_PATH,
    ]);
    assert_command_failure(&output, "Multiple equal signs in a library argument");
    assert!(output
        .stderr
        .contains("Only one equal sign `=` is allowed in the library string"));
}

/// Test that linking an object does not depend on the other objects being linked.
#[test]
fn link_output_is_independent_of_other_inputs() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let unlinked_directory = temp_dir.path().join("out");
    let reference_directory = temp_dir.path().join("ref");
    let source_path = temp_dir.path().join("x.sol");
    std::fs::copy(SOLIDITY_LINK_INDEPENDENT_OBJECTS_PATH, &source_path).unwrap();
    let library = format!(
        "{}:L=0x1111111111111111111111111111111111111111",
        source_path.to_str().unwrap()
    );

    assert_command_success(
        &execute_resolc(&[
            source_path.to_str().unwrap(),
            "--bin",
            "-o",
            unlinked_directory.to_str().unwrap(),
        ]),
        "Missing libraries should compile fine",
    );
    assert_command_success(
        &execute_resolc(&[
            source_path.to_str().unwrap(),
            "--bin",
            "-o",
            reference_directory.to_str().unwrap(),
            "--libraries",
            &library,
        ]),
        "Compiling with libraries should work",
    );

    let blob_a_path = unlinked_directory.join("x.sol:A.pvm");
    let blob_b_path = unlinked_directory.join("x.sol:B.pvm");
    let unlinked_b = std::fs::read(&blob_b_path).unwrap();

    let output = execute_resolc(&[
        "--link",
        "--libraries",
        &library,
        blob_b_path.to_str().unwrap(),
    ]);
    assert_command_success(&output, "Linking B alone should work");
    let linked_b_alone = std::fs::read(&blob_b_path).unwrap();

    std::fs::write(&blob_b_path, unlinked_b).unwrap();
    let output = execute_resolc(&[
        "--link",
        "--libraries",
        &library,
        blob_a_path.to_str().unwrap(),
        blob_b_path.to_str().unwrap(),
    ]);
    assert_command_success(&output, "Linking A and B together should work");
    let linked_b_with_a = std::fs::read(&blob_b_path).unwrap();

    let reference_b = std::fs::read(reference_directory.join("x.sol:B.pvm")).unwrap();
    assert!(
        linked_b_alone == linked_b_with_a,
        "Linking B together with A should not change B"
    );
    assert!(
        linked_b_alone == reference_b,
        "Linking B should match compile time linking"
    );
}
