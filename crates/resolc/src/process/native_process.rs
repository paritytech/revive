//! Process for compiling a single compilation unit.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use once_cell::sync::OnceCell;

use super::Input;
use super::Output;
use super::Process;

/// The overriden executable name used when the compiler is run as a library.
pub static EXECUTABLE: OnceCell<PathBuf> = OnceCell::new();

pub struct NativeProcess;

impl Process for NativeProcess {
    fn call(input: Input) -> anyhow::Result<Output> {
        let input_json = serde_json::to_vec(&input).expect("Always valid");

        let executable = match EXECUTABLE.get() {
            Some(executable) => executable.to_owned(),
            None => std::env::current_exe()?,
        };

        let mut command = Command::new(executable.as_path());
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.arg("--recursive-process");
        let process = command.spawn().map_err(|error| {
            anyhow::anyhow!("{:?} subprocess spawning error: {:?}", executable, error)
        })?;

        #[cfg(debug_assertions)]
        input
            .debug_config
            .dump_stage_output(&input.contract.path, Some("stage"), &input_json)
            .map_err(|error| {
                anyhow::anyhow!(
                    "{:?} failed to log the recursive process output: {:?}",
                    executable,
                    error,
                )
            })?;

        process
            .stdin
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{:?} stdin getting error", executable))?
            .write_all(input_json.as_slice())
            .map_err(|error| {
                anyhow::anyhow!("{:?} stdin writing error: {:?}", executable, error)
            })?;
        let output = process.wait_with_output().map_err(|error| {
            anyhow::anyhow!("{:?} subprocess output error: {:?}", executable, error)
        })?;
        if !output.status.success() {
            anyhow::bail!(
                "{}",
                String::from_utf8_lossy(output.stderr.as_slice()).to_string(),
            );
        }

        let output: Output = revive_common::deserialize_from_slice(output.stdout.as_slice())
            .map_err(|error| {
                anyhow::anyhow!(
                    "{:?} subprocess output parsing error: {}",
                    executable,
                    error,
                )
            })?;
        Ok(output)
    }
}
