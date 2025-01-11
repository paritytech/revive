pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use rstest::rstest;

/// Tests building without cloning LLVM repository.
///
/// This test verifies that the build process fails when attempting to build LLVM without
/// cloning the repository first.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build command.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
fn build_without_clone() -> anyhow::Result<()> {
    let mut cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    let file = assert_fs::NamedTempFile::new(common::LLVM_LOCK_FILE)?;
    let path = file.parent().expect("Lockfile parent dir does not exist");
    cmd.current_dir(path);
    cmd.arg("build");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("building cmake failed"))
        .stderr(predicate::str::is_match("The source directory.*does not exist").unwrap());
    Ok(())
}

/// Tests the clone, build, and clean process of the LLVM repository.
///
/// This test verifies that the LLVM repository can be successfully cloned, built, and cleaned.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build or clean commands.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[timeout(std::time::Duration::from_secs(5000))]
fn clone_build_and_clean() -> anyhow::Result<()> {
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

    let mut build_cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    build_cmd.current_dir(test_dir);
    build_cmd
        .arg("build")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success()
        .stdout(predicate::str::is_match("Installing:.*").unwrap());

    let mut builtins_cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    builtins_cmd.current_dir(test_dir);
    builtins_cmd
        .arg("builtins")
        .assert()
        .success()
        .stdout(predicate::str::is_match("Installing:.*builtins-riscv64.a").unwrap());

    let mut clean_cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    clean_cmd.current_dir(test_dir);
    clean_cmd.arg("clean");
    clean_cmd.assert().success();

    Ok(())
}

/// Tests the clone, build, and clean process of the LLVM repository for MUSL target.
///
/// This test verifies that the LLVM repository can be successfully cloned, built, and cleaned
/// with 2-staged build using MUSL as sysroot.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build or clean commands.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[timeout(std::time::Duration::from_secs(7200))]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn clone_build_and_clean_musl() -> anyhow::Result<()> {
    let lockfile = common::create_test_tmp_lockfile(None)?;
    let test_dir = lockfile
        .parent()
        .expect("Lockfile parent dir does not exist");

    Command::cargo_bin(common::REVIVE_LLVM)?
        .arg("clone")
        .current_dir(test_dir)
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .arg("--target-env")
        .arg("musl")
        .arg("build")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .current_dir(test_dir)
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir)
        .arg("builtins")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir)
        .arg("clean")
        .assert()
        .success();

    Ok(())
}

/// Tests the debug build process of the LLVM repository with tests and coverage enabled.
///
/// This test verifies that the LLVM repository can be successfully cloned and built in debug mode
/// with tests and coverage enabled.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build commands.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[timeout(std::time::Duration::from_secs(10000))]
fn debug_build_with_tests_coverage() -> anyhow::Result<()> {
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

    let mut build_cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    build_cmd.current_dir(test_dir);
    build_cmd
        .arg("build")
        .arg("--enable-coverage")
        .arg("--enable-tests")
        .arg("--build-type")
        .arg("Debug");

    build_cmd
        .assert()
        .success()
        .stdout(predicate::str::is_match("Installing:.*").unwrap());

    Ok(())
}

/// Tests LLVM build with address sanitizer enabled.
///
/// This test verifies that the LLVM repository can be successfully built with address sanitizer.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build commands.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[rstest]
#[timeout(std::time::Duration::from_secs(10000))]
fn build_with_sanitizers() -> anyhow::Result<()> {
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

    let mut build_cmd = Command::cargo_bin(common::REVIVE_LLVM)?;
    build_cmd.current_dir(test_dir);
    build_cmd.arg("build").arg("--sanitizer").arg("Address");
    build_cmd
        .assert()
        .success()
        .stdout(predicate::str::is_match("Installing:.*").unwrap());

    Ok(())
}

/// Tests the clone, build, and clean process of the LLVM repository for the emscripten target.
///
/// # Errors
///
/// Returns an error if any of the test assertions fail or if there is an error while executing
/// the build or clean commands.
///
/// # Returns
///
/// Returns `Ok(())` if the test passes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[rstest]
#[timeout(std::time::Duration::from_secs(7200))]
fn clone_build_and_clean_emscripten() -> anyhow::Result<()> {
    let lockfile = common::create_test_tmp_lockfile(None)?;
    let test_dir = lockfile
        .parent()
        .expect("Lockfile parent dir does not exist");

    let command = Command::cargo_bin(common::REVIVE_LLVM)?;
    let program = command.get_program().to_string_lossy();
    let emsdk_wrapped_build_command = format!(
        "{program} --target-env emscripten clone && \
        source {}emsdk_env.sh && \
        {program} --target-env emscripten build --llvm-projects clang --llvm-projects lld",
        revive_llvm_builder::LLVMPath::DIRECTORY_EMSDK_SOURCE,
    );

    Command::new("sh")
        .arg("-c")
        .arg(emsdk_wrapped_build_command)
        .current_dir(test_dir)
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .arg("clean")
        .current_dir(test_dir)
        .assert()
        .success();

    Ok(())
}
