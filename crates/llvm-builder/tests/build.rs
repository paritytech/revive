pub mod common;

use std::process::Command;

use assert_cmd::{cargo, prelude::*};

/// This test verifies that the LLVM repository can be successfully built and cleaned.
#[test]
fn build_and_clean() -> anyhow::Result<()> {
    let test_dir = common::TestDir::new()?;

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success();

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("builtins")
        .assert()
        .success();

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("clean")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully built and cleaned
/// with 2-staged build using MUSL as sysroot.
#[test]
#[cfg(target_os = "linux")]
fn build_and_clean_musl() -> anyhow::Result<()> {
    let test_dir = common::TestDir::new()?;

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success();

    Command::new(cargo::cargo_bin!("revive-llvm"))
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

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("clean")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully built in debug mode
/// with tests and coverage enabled.
#[test]
#[cfg(target_os = "linux")]
fn debug_build_with_tests_coverage() -> anyhow::Result<()> {
    let test_dir = common::TestDir::new()?;

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--enable-coverage")
        .arg("--enable-tests")
        .arg("--build-type")
        .arg("Debug")
        .arg("--llvm-projects")
        .arg("clang")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success();

    Ok(())
}

/// This test verifies that the LLVM repository can be successfully built with address sanitizer.
#[test]
#[cfg(target_os = "linux")]
fn build_with_sanitizers() -> anyhow::Result<()> {
    let test_dir = common::TestDir::new()?;

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--sanitizer")
        .arg("Address")
        .arg("--llvm-projects")
        .arg("lld")
        .arg("--llvm-projects")
        .arg("clang")
        .assert()
        .success();

    Ok(())
}

/// Tests the build and clean process of the LLVM repository for the emscripten target.
#[test]
#[cfg(target_os = "linux")]
fn build_and_clean_emscripten() -> anyhow::Result<()> {
    let test_dir = common::TestDir::new()?;

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .current_dir(test_dir.path())
        .arg("build")
        .arg("--llvm-projects")
        .arg("lld")
        .arg("--llvm-projects")
        .arg("clang")
        .assert()
        .success();

    // Build with emscripten target
    Command::new(common::REVIVE_LLVM)?
        .current_dir(test_dir.path())
        .arg("--target-env")
        .arg("emscripten")
        .arg("build")
        .arg("--llvm-projects")
        .arg("lld")
        .assert()
        .success();

    Command::new(cargo::cargo_bin!("revive-llvm"))
        .arg("clean")
        .current_dir(test_dir.path())
        .assert()
        .success();

    Ok(())
}
