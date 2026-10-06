//! The tests for the newyork IR stage artifacts.
//!
//! The artifacts exist so that an external tool can prove each optimization
//! pass preserved behaviour. That only holds if the exported chain is complete:
//! these tests check that every stage the manifest names was written, and that
//! the ordinals run without a gap.

use std::path::Path;

use tempfile::tempdir;

use crate::cli_utils::{
    assert_command_failure, assert_command_success, execute_resolc, CommandResult,
    SOLIDITY_CONTRACT_PATH,
};

/// The stem every artifact of the fixture contract shares.
const ARTIFACT_STEM: &str = "src_tests_data_solidity_contract.sol.C";

/// Compiles the fixture through the newyork pipeline into `directory`.
fn compile_with_stages(directory: &Path, emit_stages: bool) -> CommandResult {
    let mut arguments = vec![
        SOLIDITY_CONTRACT_PATH,
        "--newyork",
        "--overwrite",
        "--bin",
        "--debug-output-dir",
        directory.to_str().expect("a UTF-8 temporary path"),
    ];
    if emit_stages {
        arguments.push("--emit-ir-stages");
    }

    execute_resolc(&arguments)
}

/// Reads and parses the stage manifest written into `directory`.
fn read_manifest(directory: &Path) -> serde_json::Value {
    let path = directory.join(format!("{ARTIFACT_STEM}.stages.json"));
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));

    serde_json::from_str(&contents).expect("the manifest should be valid JSON")
}

#[test]
fn every_stage_the_manifest_names_was_written() {
    let directory = tempdir().unwrap();
    let result = compile_with_stages(directory.path(), true);
    assert_command_success(&result, "Emitting IR stages");

    let manifest = read_manifest(directory.path());
    let stages = manifest["stages"]
        .as_array()
        .expect("the manifest should list stages");
    assert!(!stages.is_empty(), "no stages were exported");

    for stage in stages {
        let file = stage["file"].as_str().expect("a stage names its file");
        let path = directory.path().join(file);
        assert!(path.exists(), "missing stage artifact: {}", path.display());
        assert_ne!(
            path.metadata().unwrap().len(),
            0,
            "empty stage artifact: {}",
            path.display(),
        );

        let contents = std::fs::read_to_string(&path).expect("the artifact should be readable");
        let record: serde_json::Value =
            serde_json::from_str(&contents).expect("a stage artifact should be valid JSON");
        assert_eq!(record["stage_id"], stage["id"]);
        assert_eq!(record["pass"], stage["pass"]);
        assert!(record["ir"].is_object(), "a stage carries no IR: {file}");
    }
}

/// A missing ordinal means a pass ran without being exported, which would leave
/// a consumer attributing two passes' worth of change to one.
#[test]
fn the_stage_ordinals_run_without_a_gap() {
    let directory = tempdir().unwrap();
    let result = compile_with_stages(directory.path(), true);
    assert_command_success(&result, "Emitting IR stages");

    let manifest = read_manifest(directory.path());
    let stages = manifest["stages"].as_array().expect("stages").clone();

    for (expected, stage) in stages.iter().enumerate() {
        let id = stage["id"].as_str().expect("a stage has an identifier");
        let ordinal: usize = id
            .split('.')
            .next()
            .and_then(|ordinal| ordinal.parse().ok())
            .unwrap_or_else(|| panic!("stage `{id}` does not start with an ordinal"));
        assert_eq!(ordinal, expected, "gap before stage `{id}`");
    }
}

#[test]
fn the_manifest_references_the_yul_ast() {
    let directory = tempdir().unwrap();
    let result = compile_with_stages(directory.path(), true);
    assert_command_success(&result, "Emitting IR stages");

    let manifest = read_manifest(directory.path());
    let yul_ast = manifest["yul_ast"]
        .as_str()
        .expect("the manifest should name the Yul AST");
    assert_eq!(yul_ast, format!("{ARTIFACT_STEM}.yul.json"));

    let path = directory.path().join(yul_ast);
    let contents = std::fs::read_to_string(&path).expect("the Yul AST should be readable");
    let parsed: serde_json::Value =
        serde_json::from_str(&contents).expect("the Yul AST should be valid JSON");
    assert!(parsed["identifier"].is_string(), "{contents}");
}

/// The other debug dumps must keep working with stage export off, since that is
/// every compilation that is not being verified.
#[test]
fn no_stages_are_written_without_the_flag() {
    let directory = tempdir().unwrap();
    let result = compile_with_stages(directory.path(), false);
    assert_command_success(&result, "Compiling without IR stages");

    assert!(
        directory
            .path()
            .join(format!("{ARTIFACT_STEM}.newyork"))
            .exists(),
        "the human-readable newyork dump should still be written",
    );

    let stage_files = std::fs::read_dir(directory.path())
        .expect("the directory should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.ends_with(".newyork.json")
                || name.ends_with(".stages.json")
                || name.ends_with(".yul.json")
        })
        .collect::<Vec<_>>();
    assert!(
        stage_files.is_empty(),
        "unexpected artifacts: {stage_files:?}"
    );
}

#[test]
fn stages_require_an_output_directory() {
    let result = execute_resolc(&[
        SOLIDITY_CONTRACT_PATH,
        "--newyork",
        "--bin",
        "--emit-ir-stages",
    ]);

    assert_command_failure(&result, "Emitting IR stages without an output directory");
    assert!(
        result.stderr.contains("--debug-output-dir"),
        "the error should name the missing option: {}",
        result.stderr,
    );
}
