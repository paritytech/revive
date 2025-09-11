//! Process for compiling a single compilation unit.

pub mod input;
#[cfg(not(target_os = "emscripten"))]
pub mod native_process;
pub mod output;
#[cfg(target_os = "emscripten")]
pub mod worker_process;

use std::io::{Read, Write};

use revive_solc_json_interface::standard_json::output::error::source_location::SourceLocation;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use serde::de::DeserializeOwned;
use serde::Serialize;

use self::input::Input;
use self::output::Output;

pub trait Process {
    /// Read input from `stdin`, compile a contract, and write the output to `stdout`.
    fn run() -> anyhow::Result<()> {
        let input_json = std::io::read_to_string(std::io::stdin())
            .map_err(|error| anyhow::anyhow!("Stdin reading error: {error}"))?;
        let input: Input = revive_common::deserialize_from_str(input_json.as_str())
            .map_err(|error| anyhow::anyhow!("Stdin parsing error: {error}"))?;

        let source_location = SourceLocation::new(input.contract.identifier.path);

        let result = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                input
                    .contract
                    .compile(
                        input.identifier_paths,
                        input.missing_libraries,
                        input.factory_dependencies,
                        input.metadata_hash_type,
                        input.append_cbor,
                        input.optimizer_settings,
                        input.llvm_options,
                        input.output_assembly,
                        input.debug_config,
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

        unsafe { inkwell::support::shutdown_llvm() };
        Ok(())
    }

    /// Runs this process recursively to compile a single contract.
    fn call<I: Serialize, O: DeserializeOwned>(
        path: &str,
        input: I,
    ) -> Result<O, SolcStandardJsonOutputError>;
}
