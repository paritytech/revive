//! The revive explorer leverages debug info to get insights into emitted code.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub static OBJDUMP_EXECUTABLE: &str = "llvm-objdump";
pub static OBJDUMP_ARGUMENTS: [&str; 2] = ["-S", "-l"];

/// Calls `llvm-objdump` to extract instructions and debug info from
/// the shared object at `path`. If `objdump` is `Some` then this is
/// what will be called.
pub fn objdump(path: &Path, objdump: Option<PathBuf>) -> anyhow::Result<String> {
    let executable = objdump
        .to_owned()
        .unwrap_or_else(|| PathBuf::from(OBJDUMP_EXECUTABLE));

    let output = Command::new(executable)
        .args(&OBJDUMP_ARGUMENTS)
        .arg(path)
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
