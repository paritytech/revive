//! Process for compiling a single compilation unit using Web Workers.

use std::ffi::{c_char, c_void, CStr, CString};
use std::fs::File;
use std::io::Read;
use std::io::Write;

use super::Input;
use super::Output;
use super::Process;

use anyhow::Context;
use serde::Deserialize;

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
    /// Read input from `stdin`, compile a contract, and write the output to `stdout`.
    fn run(input_file: Option<&mut std::fs::File>) -> anyhow::Result<()> {
        let mut buffer = Vec::with_capacity(16384);
        // TODO: Init correctly stdin in emscripten - preload FS conf before module init
        let mut stdin = File::open("/in")
            .map_err(|error| anyhow::anyhow!("File /in openning error: {}", error))?;
        let mut stdout = File::create("/out")
            .map_err(|error| anyhow::anyhow!("File /out creating error: {}", error))?;
        let mut stderr = File::create("/err")
            .map_err(|error| anyhow::anyhow!("File /err creating error: {}", error))?;

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
        let result = input.contract.compile(
            input.project,
            input.optimizer_settings,
            input.include_metadata_hash,
            input.debug_config,
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
    fn call(input: Input) -> anyhow::Result<Output> {
        let input_json = serde_json::to_vec(&input).expect("Always valid");
        let input_str = String::from_utf8(input_json).expect("Input shall be valid");
        // Prepare the input string for the Emscripten function
        let input_cstring = CString::new(input_str).expect("CString allocation failed");

        // Call the Emscripten function
        let output_ptr =
            unsafe { resolc_compile(input_cstring.as_ptr(), input_cstring.as_bytes().len()) };

        // Convert the output pointer back to a Rust string
        let output_str = unsafe {
            CStr::from_ptr(output_ptr)
                .to_str()
                .with_context(|| "Failed to convert C string to Rust string")
                .map(str::to_owned)
        };
        unsafe { libc::free(output_ptr as *mut c_void) };
        let output_str = output_str?;
        let response: Response = serde_json::from_str(&output_str)
            .map_err(|error| anyhow::anyhow!("Worker output parsing error: {}", error,))?;
        match response {
            Response::Success(out) => {
                let output: Output = revive_common::deserialize_from_slice(out.data.as_bytes())
                    .map_err(|error| {
                        anyhow::anyhow!("resolc.js subprocess output parsing error: {}", error,)
                    })?;

                Ok(output)
            }
            Response::Error(err) => anyhow::bail!("Worker error: {}", err.message,),
        }
    }
}

extern "C" {
    fn resolc_compile(input_ptr: *const c_char, input_len: usize) -> *const c_char;
}
