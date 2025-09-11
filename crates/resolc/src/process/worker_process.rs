//! Process for compiling a single compilation unit using Web Workers.

use std::ffi::{c_char, c_void, CStr, CString};

use super::Input;
use super::Output;
use super::Process;

use revive_solc_json_interface::SolcStandardJsonOutputError;

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
    fn call<I, O>(path: &str, input: I) -> Result<O, SolcStandardJsonOutputError>
    where
        I: serde::Serialize,
        O: serde::de::DeserializeOwned,
    {
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
