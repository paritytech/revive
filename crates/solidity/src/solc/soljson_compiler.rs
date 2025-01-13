//! The Solidity compiler.

use std::path::Path;
use std::path::PathBuf;

use crate::solc::combined_json::CombinedJson;
use crate::solc::pipeline::Pipeline;
use crate::solc::standard_json::input::Input as StandardJsonInput;
use crate::solc::standard_json::output::Output as StandardJsonOutput;
use crate::solc::version::Version;
use anyhow::Context;
use std::ffi::{c_char, c_void, CStr, CString};

use super::Compiler;

extern "C" {
    fn soljson_version() -> *const c_char;
    fn soljson_compile(inputPtr: *const c_char, inputLen: usize) -> *const c_char;
}

/// The Solidity compiler.
pub struct SoljsonCompiler {
    /// The lazily-initialized compiler version.
    pub version: Option<Version>,
}

impl Compiler for SoljsonCompiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &mut self,
        mut input: StandardJsonInput,
        pipeline: Pipeline,
        _base_path: Option<String>,
        _include_paths: Vec<String>,
        _allow_paths: Option<String>,
    ) -> anyhow::Result<StandardJsonOutput> {
        let version = self.version()?;
        input.normalize(&version.default);
        let suppressed_warnings = input.suppressed_warnings.take().unwrap_or_default();

        let input_json = serde_json::to_vec(&input).expect("Always valid");
        let out = Self::compile_standard_json(String::from_utf8_lossy(&input_json).to_string())
            .map_err(|error| anyhow::anyhow!("Soljson subprocess error: {:?}", error))?;

        let mut output: StandardJsonOutput = revive_common::deserialize_from_slice(out.as_bytes())
            .map_err(|error| {
                anyhow::anyhow!(
                    "Soljson subprocess output parsing error: {}\n{}",
                    error,
                    revive_common::deserialize_from_slice::<serde_json::Value>(out.as_bytes())
                        .map(|json| serde_json::to_string_pretty(&json).expect("Always valid"))
                        .unwrap_or_else(|_| String::from_utf8_lossy(out.as_bytes()).to_string()),
                )
            })?;

        output.preprocess_ast(&version, pipeline, suppressed_warnings.as_slice())?;

        Ok(output)
    }

    fn combined_json(
        &self,
        _paths: &[PathBuf],
        _combined_json_argument: &str,
    ) -> anyhow::Result<CombinedJson> {
        unimplemented!();
    }

    fn validate_yul(&self, _path: &Path) -> anyhow::Result<()> {
        unimplemented!();
    }

    fn version(&mut self) -> anyhow::Result<Version> {
        let version = Self::get_soljson_version()?;
        let long = version.clone();
        let default: semver::Version = version
            .split('+')
            .next()
            .ok_or_else(|| anyhow::anyhow!("Soljson version parsing: metadata dropping"))?
            .parse()
            .map_err(|error| anyhow::anyhow!("Soljson version parsing: {}", error))?;

        let l2_revision: Option<semver::Version> = version
            .split('-')
            .nth(1)
            .and_then(|version| version.parse().ok());

        let version = Version::new(long, default, l2_revision);
        if version.default < super::FIRST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`Soljson` versions <{} are not supported, found {}",
                super::FIRST_SUPPORTED_VERSION,
                version.default
            );
        }
        if version.default > super::LAST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`Soljson` versions >{} are not supported, found {}",
                super::LAST_SUPPORTED_VERSION,
                version.default
            );
        }

        self.version = Some(version.clone());

        Ok(version)
    }
}

impl SoljsonCompiler {
    fn get_soljson_version() -> anyhow::Result<String> {
        unsafe {
            let version_ptr = soljson_version();
            let version = CStr::from_ptr(version_ptr)
                .to_str()
                .with_context(|| "Failed to convert C string to Rust string")
                .map(str::to_owned);
            libc::free(version_ptr as *mut c_void);
            Ok(version?)
        }
    }

    fn compile_standard_json(input: String) -> anyhow::Result<String> {
        let c_input = CString::new(input).unwrap();
        let c_input_len = c_input.as_bytes().len();

        unsafe {
            let output_ptr = soljson_compile(c_input.as_ptr(), c_input_len);
            let output_json = CStr::from_ptr(output_ptr)
                .to_str()
                .with_context(|| "Failed to convert C string to Rust string")
                .map(str::to_owned);
            libc::free(output_ptr as *mut c_void);
            Ok(output_json?)
        }
    }
}
