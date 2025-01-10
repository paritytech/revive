pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use rstest::rstest;

/// Tests the cloning process of the LLVM repository using a specific branch and reference.
///
/// This test verifies that the LLVM repository can be successfully cloned using a specific branch
/// and reference.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the clone command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn clone() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let lockfile = common::create_test_tmp_lockfile(None)?;
    let test_dir = lockfile
        .parent()
        .expect("Lockfile parent dir does not exist");
    cmd.current_dir(test_dir);
    cmd.arg("clone");
    cmd.assert()
        .success()
        .stderr(predicate::str::is_match(".*Updating files:.*100%.*done").unwrap());
    Ok(())
}

/// Tests the full cloning process of the LLVM repository using a specific branch and reference.
///
/// This test verifies that the LLVM repository can be successfully cloned using a specific branch
/// and reference with --deep option.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the clone command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn clone_deep() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let lockfile = common::create_test_tmp_lockfile(None)?;
    let test_dir = lockfile
        .parent()
        .expect("Lockfile parent dir does not exist");
    cmd.current_dir(test_dir);
    cmd.arg("clone");
    cmd.arg("--deep");
    cmd.assert()
        .success()
        .stderr(predicate::str::is_match(".*Updating files:.*100%.*done").unwrap());
    Ok(())
}

/// Tests the cloning process of the LLVM repository using an invalid reference.
///
/// This test verifies that attempting to clone the LLVM repository using an invalid reference
/// results in a failure.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the clone command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn clone_wrong_reference() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let lockfile = common::create_test_tmp_lockfile(Some(
        common::REVIVE_LLVM_REPO_TEST_SHA_INVALID.to_string(),
    ))?;
    let test_dir = lockfile
        .parent()
        .expect("Lockfile parent dir does not exist");
    cmd.current_dir(test_dir);
    cmd.arg("clone");
    cmd.assert().failure().stderr(predicate::str::contains(
        "Error: LLVM repository commit checking out failed",
    ));
    Ok(())
}

/// Tests the cloning process of the LLVM repository without a lock file.
///
/// This test verifies that attempting to clone the LLVM repository without a lock file
/// results in a failure.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the clone command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn clone_without_lockfile() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let file = assert_fs::NamedTempFile::new(common::LLVM_LOCK_FILE)?;
    let path = file.parent().expect("Lockfile parent dir does not exist");
    cmd.current_dir(path);
    cmd.arg("clone");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "Error: Error opening \"{}\" file",
            common::LLVM_LOCK_FILE
        )));
    Ok(())
}
