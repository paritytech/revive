//! The `llvm-dwarfdump` utility helper library.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub static EXECUTABLE: &str = "llvm-dwarfdump";
pub static DEBUG_LINES_ARGUMENTS: [&str; 1] = ["--debug-line"];
pub static SOURCE_FILE_ARGUMENTS: [&str; 1] = ["--show-sources"];

/// Calls the `llvm-dwarfdump` tool to extract debug line information
/// from the shared object at `path`. Returns the output.
///
/// Provide `Some(dwarfdump_exectuable)` to override the default executable.
pub fn debug_lines(
    shared_object: &Path,
    dwarfdump_executable: &Option<PathBuf>,
) -> anyhow::Result<String> {
    dwarfdump(shared_object, dwarfdump_executable, &DEBUG_LINES_ARGUMENTS)
}

/// Calls the `llvm-dwarfdump` tool to extract the source file name.
/// Returns the source file path.
///
/// Provide `Some(dwarfdump_exectuable)` to override the default executable.
pub fn source_file(
    shared_object: &Path,
    dwarfdump_executable: &Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    let output = dwarfdump(shared_object, dwarfdump_executable, &SOURCE_FILE_ARGUMENTS)?;
    let output = output.trim();

    if output.is_empty() {
        anyhow::bail!(
            "the shared object at path `{}` doesn't contain the source file name. Hint: compile with debug information (-g)?",
            shared_object.display()
        );
    }

    Ok(output.into())
}

/// The internal `llvm-dwarfdump` helper function.
fn dwarfdump(
    shared_object: &Path,
    dwarfdump_executable: &Option<PathBuf>,
    arguments: &[&str],
) -> anyhow::Result<String> {
    let executable = dwarfdump_executable
        .to_owned()
        .unwrap_or_else(|| PathBuf::from(EXECUTABLE));

    let output = Command::new(executable)
        .args(arguments)
        .arg(shared_object)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    if !output.status.success() {
        anyhow::bail!(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
