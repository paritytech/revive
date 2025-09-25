//! Process for compiling a single compilation unit using Web Workers.

use std::ffi::{c_char, c_void, CStr, CString};

use serde::Deserialize;

use revive_solc_json_interface::standard_json::output::error::source_location::SourceLocation;
use revive_solc_json_interface::SolcStandardJsonOutputError;

use super::Input;
use super::Output;
use super::Process;

#[derive(Deserialize)]
struct Error {
    message: String,
}

#[derive(Deserialize)]
struct Success {
    data: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Success(Success),
    Error(Error),
}

pub struct WorkerProcess;

impl Process for WorkerProcess {
    fn run(input: Input) -> anyhow::Result<()> {
        let source_location = SourceLocation::new(input.contract.identifier.path.to_owned());

        let result = input
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
            });

        serde_json::to_writer(std::io::stdout(), &result)
            .map_err(|error| anyhow::anyhow!("Stdout writing error: {error}"))?;

        Ok(())
    }

    fn call<I, O>(_path: &str, input: I) -> Result<O, SolcStandardJsonOutputError>
    where
        I: serde::Serialize,
        O: serde::de::DeserializeOwned,
    {
        let input_json = serde_json::to_vec(&input).expect("Always valid");
        let input_str = String::from_utf8(input_json).expect("Input shall be valid");
        let input_cstring = CString::new(input_str).expect("CString allocation failed");

        // Call the Emscripten function
        let output_ptr =
            unsafe { resolc_compile(input_cstring.as_ptr(), input_cstring.as_bytes().len()) };

        // Convert the output pointer back to a Rust string
        let output_str = unsafe { CStr::from_ptr(output_ptr).to_str().map(str::to_owned) };
        unsafe { libc::free(output_ptr as *mut c_void) };

        let output_str = output_str.unwrap_or_else(|error| panic!("resolc.js output: {error:?}"));
        let response = serde_json::from_str(&output_str)
            .unwrap_or_else(|error| panic!("Worker output parsing error: {error}"));
        match response {
            Response::Success(out) => {
                match revive_common::deserialize_from_slice(out.data.as_bytes()) {
                    Ok(output) => output,
                    Err(error) => {
                        panic!("resolc.js subprocess output parsing error: {error}")
                    }
                }
            }
            Response::Error(err) => panic!("Worker error: {}", err.message),
        }
    }
}

extern "C" {
    fn resolc_compile(input_ptr: *const c_char, input_len: usize) -> *const c_char;
}
