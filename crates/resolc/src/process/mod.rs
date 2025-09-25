//! Process for compiling a single compilation unit.

pub mod input;
#[cfg(not(target_os = "emscripten"))]
pub mod native_process;
pub mod output;
#[cfg(target_os = "emscripten")]
pub mod worker_process;

use revive_solc_json_interface::SolcStandardJsonOutputError;
use serde::de::DeserializeOwned;
use serde::Serialize;

use self::input::Input;
use self::output::Output;

pub trait Process {
    /// Read input from `stdin`, compile a contract, and write the output to `stdout`.
    fn run(input: Input) -> anyhow::Result<()>;

    /// Runs this process recursively to compile a single contract.
    fn call<I: Serialize, O: DeserializeOwned>(
        path: &str,
        input: I,
    ) -> Result<O, SolcStandardJsonOutputError>;
}
