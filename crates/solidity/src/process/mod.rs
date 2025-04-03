//! Process for compiling a single compilation unit.

pub mod input;
#[cfg(not(target_os = "emscripten"))]
pub mod native_process;
pub mod output;
#[cfg(target_os = "emscripten")]
pub mod worker_process;

use std::io::{Read, Write};

use self::input::Input;
use self::output::Output;

pub trait Process {
    /// Read input from `stdin`, compile a contract, and write the output to `stdout`.
    fn run(input_file: Option<&mut std::fs::File>) -> anyhow::Result<()> {
        let mut stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();

        let mut buffer = Vec::with_capacity(16384);
        match input_file {
            Some(ins) => {
                if let Err(error) = ins.read_to_end(&mut buffer) {
                    anyhow::bail!("Failed to read recursive process input file: {:?}", error);
                }
            }
            None => {
                if let Err(error) = stdin.read_to_end(&mut buffer) {
                    anyhow::bail!(
                        "Failed to read recursive process input from stdin: {:?}",
                        error
                    )
                }
            }
        }

        let input: Input = revive_common::deserialize_from_slice(buffer.as_slice())?;

        revive_llvm_context::initialize_llvm(
            revive_llvm_context::Target::PVM,
            crate::DEFAULT_EXECUTABLE_NAME,
            &input.llvm_arguments,
        );

        let result = input.contract.compile(
            input.project,
            input.optimizer_settings,
            input.include_metadata_hash,
            input.debug_config,
            &input.llvm_arguments,
        );

        match result {
            Ok(build) => {
                let output = Output::new(build);
                let json = serde_json::to_vec(&output).expect("Always valid");
                stdout
                    .write_all(json.as_slice())
                    .expect("Stdout writing error");
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                stderr
                    .write_all(message.as_bytes())
                    .expect("Stderr writing error");
                Err(error)
            }
        }
    }

    /// Runs this process recursively to compile a single contract.
    fn call(input: Input) -> anyhow::Result<Output>;
}
