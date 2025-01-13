pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;

/// This test verifies that the LLVM repository can be successfully cloned using a specific branch
/// and reference.
#[test]
fn clone() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully cloned using a specific branch
/// and reference with --deep option.
#[test]
fn clone_deep() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .arg("--deep")
        .assert()
        .success();

    Ok(())
}
