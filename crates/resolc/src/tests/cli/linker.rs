use crate::tests::cli::utils::{assert_command_success, execute_resolc, DEPENDENCY_CONTRACT_PATH};

/// Test deploy time linking a contract with unresolved factory dependencies.
#[test]
fn deploy_time_linking_works() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let output_directory = temp_dir.path().to_path_buf();
    let source_path = temp_dir.path().to_path_buf().join("dependency.sol");
    std::fs::copy(DEPENDENCY_CONTRACT_PATH, &source_path).unwrap();

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
    let output = execute_resolc(&[DEPENDENCY_CONTRACT_PATH, "--bin"]);
    assert_command_success(&output, "Missing libraries should compile fine");
    assert!(output.stderr.contains("is unlinked"));
}
