//! Process for compiling a single compilation unit.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;
use std::sync::OnceLock;

use revive_common::deserialize_from_slice;
use revive_common::EXIT_CODE_SUCCESS;
use revive_solc_json_interface::standard_json::output::error::source_location::SourceLocation;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::Input;
use super::Output;
use super::Process;

/// The default executable path, lazily initialized from the current binary.
static DEFAULT_EXECUTABLE: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::current_exe().expect("Should have an executable"));

/// Override for the executable path, used when the compiler is run as a library.
pub static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();

pub struct NativeProcess;

impl Process for NativeProcess {
    fn run(input: Input) -> anyhow::Result<()> {
        let source_location = SourceLocation::new(input.contract.identifier.path.to_owned());

        let result = std::thread::Builder::new()
            .stack_size(crate::RAYON_WORKER_STACK_SIZE)
            .spawn(move || {
                input
                    .contract
                    .compile(
                        input.solc_version,
                        input.optimizer_settings,
                        input.metadata_hash,
                        input.debug_config,
                        &input.llvm_arguments,
                        input.memory_config,
                        input.missing_libraries,
                        input.factory_dependencies,
                        input.identifier_paths,
                    )
                    .map(Output::new)
                    .map_err(|error| {
                        SolcStandardJsonOutputError::new_error(error, Some(source_location), None)
                    })
            })
            .expect("Threading error")
            .join()
            .expect("Threading error");

        serde_json::to_writer(std::io::stdout(), &result)
            .map_err(|error| anyhow::anyhow!("Stdout writing error: {error}"))?;

        Ok(())
    }

    fn call<I, O>(path: &str, input: I) -> Result<O, SolcStandardJsonOutputError>
    where
        I: Serialize,
        O: DeserializeOwned,
    {
        let executable = EXECUTABLE.get().unwrap_or(&DEFAULT_EXECUTABLE);
        let mut command = Command::new(executable.as_path());
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::inherit());
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

        if result.status.code() != Some(EXIT_CODE_SUCCESS) {
            let message = format!(
                "{executable:?} subprocess failed with exit code {:?}:\n{}",
                result.status.code(),
                String::from_utf8_lossy(result.stdout.as_slice()),
            );
            return Err(SolcStandardJsonOutputError::new_error(
                message,
                Some(SourceLocation::new(path.to_owned())),
                None,
            ));
        }

        match deserialize_from_slice(result.stdout.as_slice()) {
            Ok(output) => output,
            Err(error) => {
                panic!(
                    "{executable:?} subprocess stdout parsing error: {error:?}\n{}",
                    String::from_utf8_lossy(result.stdout.as_slice()),
                );
            }
        }
    }
}
