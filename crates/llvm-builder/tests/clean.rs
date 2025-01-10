pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use rstest::rstest;

/// Tests the clean process without cloning the LLVM repository.
///
/// This test verifies that attempting to clean the LLVM directory without first cloning it
/// results in a failure.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the clean command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn clean_without_clone() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let file = assert_fs::NamedTempFile::new(common::LLVM_LOCK_FILE)?;
    let path = file.parent().expect("Lockfile parent dir does not exist");
    cmd.current_dir(path);
    cmd.arg("clean");
    cmd.assert().failure().stderr(predicate::str::contains(
        "Unable to remove target LLVM directory",
    ));
    Ok(())
}
