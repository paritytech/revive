pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

/// Tests the version command.
#[test]
fn version() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(common::PACKAGE_VERSION));
    Ok(())
}
