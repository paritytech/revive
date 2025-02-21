pub mod common;

use std::process::Command;

use assert_cmd::prelude::*;

/// This test verifies that the LLVM repository can be successfully cloned, built, and cleaned.
#[test]
fn clone_build_and_clean() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("builtins")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clean")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully cloned, built, and cleaned
/// with 2-staged build using MUSL as sysroot.
#[test]
#[cfg(target_os = "linux")]
fn clone_build_and_clean_musl() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .arg("clone")
        .current_dir(test_dir.path())
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
        .current_dir(test_dir.path())
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("builtins")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clean")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully cloned and built in debug mode
/// with tests and coverage enabled.
#[test]
#[cfg(target_os = "linux")]
fn debug_build_with_tests_coverage() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--enable-coverage")
        .arg("--enable-tests")
        .arg("--build-type")
        .arg("Debug")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully built with address sanitizer.
#[test]
#[cfg(target_os = "linux")]
fn build_with_sanitizers() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("clone")
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--sanitizer")
        .arg("Address")
        .assert()
        .success();

    Ok(())
}

/// Tests the clone, build, and clean process of the LLVM repository for the emscripten target.
#[test]
#[cfg(target_os = "linux")]
fn clone_build_and_clean_emscripten() -> anyhow::Result<()> {
    let test_dir = common::TestDir::with_lockfile(None)?;
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
        .current_dir(test_dir.path())
        .assert()
        .success();

    Command::cargo_bin(common::REVIVE_LLVM)?
        .arg("clean")
        .current_dir(test_dir.path())
        .assert()
        .success();

    Ok(())
}
