//! Process for compiling a single compilation unit.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use once_cell::sync::OnceCell;
use revive_solc_json_interface::standard_json::output::error::source_location::SourceLocation;
use revive_solc_json_interface::SolcStandardJsonOutputError;

use super::Process;

/// The overriden executable name used when the compiler is run as a library.
pub static EXECUTABLE: OnceCell<PathBuf> = OnceCell::new();

pub struct NativeProcess;

impl Process for NativeProcess {
    fn call<I, O>(path: &str, input: I) -> Result<O, SolcStandardJsonOutputError>
    where
        I: serde::Serialize,
        O: serde::de::DeserializeOwned,
    {
        let executable = EXECUTABLE
            .get()
            .cloned()
            .unwrap_or_else(|| std::env::current_exe().expect("Should have an executable"));
        let mut command = Command::new(executable.as_path());
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.arg("--recursive-process");
        command.arg(path);

        let mut process = command
            .spawn()
            .unwrap_or_else(|error| panic!("{executable:?} subprocess spawning: {error:?}"));
        let stdin = process
            .stdin
            .as_mut()
            .unwrap_or_else(|| panic!("{executable:?} subprocess stdin getting error"));
        let stdin_input = serde_json::to_vec(&input).expect("Always valid");
        stdin
            .write_all(stdin_input.as_slice())
            .unwrap_or_else(|error| panic!("{executable:?} subprocess stdin writing: {error:?}"));

        let result = process
            .wait_with_output()
            .unwrap_or_else(|error| panic!("{executable:?} subprocess output reading: {error:?}"));

        if result.status.code() != Some(revive_common::EXIT_CODE_SUCCESS) {
            let message = format!(
                "{executable:?} subprocess failed with exit code {:?}:\n{}\n{}",
                result.status.code(),
                String::from_utf8_lossy(result.stdout.as_slice()),
                String::from_utf8_lossy(result.stderr.as_slice()),
            );
            return Err(SolcStandardJsonOutputError::new_error(
                message,
                Some(SourceLocation::new(path.to_owned())),
                None,
            ));
        }

        match revive_common::deserialize_from_slice(result.stdout.as_slice()) {
            Ok(output) => output,
            Err(error) => {
                panic!(
                    "{executable:?} subprocess stdout parsing error: {error:?}\n{}\n{}",
                    String::from_utf8_lossy(result.stdout.as_slice()),
                    String::from_utf8_lossy(result.stderr.as_slice()),
                );
            }
        }
    }
}
