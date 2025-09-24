//! Process for compiling a single compilation unit.

pub mod input;
#[cfg(not(target_os = "emscripten"))]
pub mod native_process;
pub mod output;
#[cfg(target_os = "emscripten")]
pub mod worker_process;

use revive_llvm_context::Target;
use revive_solc_json_interface::standard_json::output::error::source_location::SourceLocation;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use serde::de::DeserializeOwned;
use serde::Serialize;

use self::input::Input;
use self::output::Output;

pub trait Process {
    /// Read input from `stdin`, compile a contract, and write the output to `stdout`.
    fn run(input: Input) -> anyhow::Result<()> {
        let source_location = SourceLocation::new(input.contract.identifier.path.to_owned());

        let result = std::thread::Builder::new()
            .stack_size(crate::RAYON_WORKER_STACK_SIZE)
            .spawn(move || {
                input
                    .contract
                    .compile(
                        None,
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

    /// Runs this process recursively to compile a single contract.
    fn call<I: Serialize, O: DeserializeOwned>(
        path: &str,
        input: I,
    ) -> Result<O, SolcStandardJsonOutputError>;
}
