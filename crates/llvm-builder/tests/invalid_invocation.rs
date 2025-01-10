pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use rstest::rstest;

/// Tests invalid options for various subcommands.
///
/// This test verifies that providing invalid options for different subcommands results in a failure.
///
/// # Parameters
///
/// - `subcommand`: The subcommand being tested.
/// - `option`: The invalid option being tested.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[case("", "--invalid-option")]
#[case("build", "--invalid-build-option")]
#[case("clean", "--invalid-clean-option")]
#[case("clone", "--invalid-clone-option")]
#[case("checkout", "--invalid-checkout-option")]
fn invalid_option(#[case] subcommand: &str, #[case] option: &str) -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    if subcommand.is_empty() {
        cmd.arg(subcommand);
    }
    cmd.arg(option);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "error: unexpected argument \'{}\' found",
            option
        )));
    Ok(())
}

/// Tests invalid subcommands.
///
/// This test verifies that providing invalid subcommands results in a failure.
///
/// # Parameters
///
/// - `subcommand`: The invalid subcommand being tested.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[case("invalid-subcommand")]
#[case("123")]
#[case("$$.@!;-a3")]
fn invalid_subcommand(#[case] subcommand: &str) -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    cmd.arg(subcommand);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "error: unrecognized subcommand \'{}\'",
            subcommand
        )));
    Ok(())
}
