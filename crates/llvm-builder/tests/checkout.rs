pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;

/// This test verifies that after cloning the LLVM repository, checking out a specific branch
/// or reference works as expected.
#[test]
fn checkout_after_clone() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("checkout")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that after cloning the LLVM repository, checking out a specific branch
/// or reference with the `--force` option works as expected.
#[test]
fn force_checkout() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("checkout")
        .arg("--force")
        .assert()
        .success();

    Ok(())
}
