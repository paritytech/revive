//! The Solidity compiler.

use crate::solc::combined_json::CombinedJson;
use crate::solc::pipeline::Pipeline;
use crate::solc::standard_json::input::Input as StandardJsonInput;
use crate::solc::standard_json::output::Output as StandardJsonOutput;
use crate::solc::version::Version;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use super::Compiler;
use crate::solc::{FIRST_SUPPORTS_BASE_PATH, FIRST_SUPPORTS_INCLUDE_PATH};
/// The Solidity compiler.
pub struct SolcCompiler {
    /// The binary executable name.
    pub executable: String,
    /// The lazily-initialized compiler version.
    pub version: Option<Version>,
}

impl SolcCompiler {
    /// The default executable name.
    pub const DEFAULT_EXECUTABLE_NAME: &'static str = "solc";

    /// A shortcut constructor.
    /// Different tools may use different `executable` names. For example, the integration tester
    /// uses `solc-<version>` format.
    pub fn new(executable: String) -> anyhow::Result<Self> {
        if let Err(error) = which::which(executable.as_str()) {
            anyhow::bail!(
                "The `{executable}` executable not found in ${{PATH}}: {}",
                error
            );
        }
        Ok(Self {
            executable,
            version: None,
        })
    }
}

impl Compiler for SolcCompiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &mut self,
        mut input: StandardJsonInput,
        pipeline: Pipeline,
        base_path: Option<String>,
        include_paths: Vec<String>,
        allow_paths: Option<String>,
    ) -> anyhow::Result<StandardJsonOutput> {
        let version = self.version()?;

        let mut command = std::process::Command::new(self.executable.as_str());
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.arg("--standard-json");

        if let Some(base_path) = &base_path {
            if !FIRST_SUPPORTS_BASE_PATH.matches(&version.default) {
                anyhow::bail!(
                    "--base-path not supported this version {} of solc",
                    &version.default
                );
            }
            command.arg("--base-path").arg(base_path);
        }

        if !include_paths.is_empty() && !FIRST_SUPPORTS_INCLUDE_PATH.matches(&version.default) {
            anyhow::bail!(
                "--include-path not supported this version {} of solc",
                &version.default
            );
        }

        for include_path in include_paths.into_iter() {
            command.arg("--include-path");
            command.arg(include_path);
        }
        if let Some(allow_paths) = allow_paths {
            command.arg("--allow-paths");
            command.arg(allow_paths);
        }

        input.normalize(&version.default);

        let suppressed_warnings = input.suppressed_warnings.take().unwrap_or_default();

        let input_json = serde_json::to_vec(&input).expect("Always valid");

        let process = command.spawn().map_err(|error| {
            anyhow::anyhow!("{} subprocess spawning error: {:?}", self.executable, error)
        })?;
        process
            .stdin
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{} stdin getting error", self.executable))?
            .write_all(input_json.as_slice())
            .map_err(|error| {
                anyhow::anyhow!("{} stdin writing error: {:?}", self.executable, error)
            })?;

        let output = process.wait_with_output().map_err(|error| {
            anyhow::anyhow!("{} subprocess output error: {:?}", self.executable, error)
        })?;
        if !output.status.success() {
            anyhow::bail!(
                "{} error: {}",
                self.executable,
                String::from_utf8_lossy(output.stderr.as_slice()).to_string()
            );
        }

        let mut output: StandardJsonOutput =
            revive_common::deserialize_from_slice(output.stdout.as_slice()).map_err(|error| {
                anyhow::anyhow!(
                    "{} subprocess output parsing error: {}\n{}",
                    self.executable,
                    error,
                    revive_common::deserialize_from_slice::<serde_json::Value>(
                        output.stdout.as_slice()
                    )
                    .map(|json| serde_json::to_string_pretty(&json).expect("Always valid"))
                    .unwrap_or_else(
                        |_| String::from_utf8_lossy(output.stdout.as_slice()).to_string()
                    ),
                )
            })?;
        output.preprocess_ast(&version, pipeline, suppressed_warnings.as_slice())?;

        Ok(output)
    }

    /// The `solc --combined-json abi,hashes...` mirror.
    fn combined_json(
        &self,
        paths: &[PathBuf],
        combined_json_argument: &str,
    ) -> anyhow::Result<CombinedJson> {
        let mut command = std::process::Command::new(self.executable.as_str());
        command.args(paths);

        let mut combined_json_flags = Vec::new();
        let mut combined_json_fake_flag_pushed = false;
        let mut filtered_flags = Vec::with_capacity(3);
        for flag in combined_json_argument.split(',') {
            match flag {
                flag @ "asm" | flag @ "bin" | flag @ "bin-runtime" => filtered_flags.push(flag),
                flag => combined_json_flags.push(flag),
            }
        }
        if combined_json_flags.is_empty() {
            combined_json_flags.push("ast");
            combined_json_fake_flag_pushed = true;
        }
        command.arg("--combined-json");
        command.arg(combined_json_flags.join(","));

        let output = command.output().map_err(|error| {
            anyhow::anyhow!("{} subprocess error: {:?}", self.executable, error)
        })?;
        if !output.status.success() {
            println!("{}", String::from_utf8_lossy(output.stdout.as_slice()));
            println!("{}", String::from_utf8_lossy(output.stderr.as_slice()));
            anyhow::bail!(
                "{} error: {}",
                self.executable,
                String::from_utf8_lossy(output.stdout.as_slice()).to_string()
            );
        }

        let mut combined_json: CombinedJson =
            revive_common::deserialize_from_slice(output.stdout.as_slice()).map_err(|error| {
                anyhow::anyhow!(
                    "{} subprocess output parsing error: {}\n{}",
                    self.executable,
                    error,
                    revive_common::deserialize_from_slice::<serde_json::Value>(
                        output.stdout.as_slice()
                    )
                    .map(|json| serde_json::to_string_pretty(&json).expect("Always valid"))
                    .unwrap_or_else(
                        |_| String::from_utf8_lossy(output.stdout.as_slice()).to_string()
                    ),
                )
            })?;
        for filtered_flag in filtered_flags.into_iter() {
            for (_path, contract) in combined_json.contracts.iter_mut() {
                match filtered_flag {
                    "asm" => contract.asm = Some(serde_json::Value::Null),
                    "bin" => contract.bin = Some("".to_owned()),
                    "bin-runtime" => contract.bin_runtime = Some("".to_owned()),
                    _ => continue,
                }
            }
        }
        if combined_json_fake_flag_pushed {
            combined_json.source_list = None;
            combined_json.sources = None;
        }
        combined_json.remove_evm();

        Ok(combined_json)
    }

    /// The `solc` Yul validator.
    fn validate_yul(&self, path: &Path) -> anyhow::Result<()> {
        let mut command = std::process::Command::new(self.executable.as_str());
        command.arg("--strict-assembly");
        command.arg(path);

        let output = command.output().map_err(|error| {
            anyhow::anyhow!("{} subprocess error: {:?}", self.executable, error)
        })?;
        if !output.status.success() {
            anyhow::bail!(
                "{} error: {}",
                self.executable,
                String::from_utf8_lossy(output.stderr.as_slice()).to_string()
            );
        }

        Ok(())
    }

    /// The `solc --version` mini-parser.
    fn version(&mut self) -> anyhow::Result<Version> {
        if let Some(version) = self.version.as_ref() {
            return Ok(version.to_owned());
        }

        let mut command = std::process::Command::new(self.executable.as_str());
        command.arg("--version");
        let output = command.output().map_err(|error| {
            anyhow::anyhow!("{} subprocess error: {:?}", self.executable, error)
        })?;
        if !output.status.success() {
            anyhow::bail!(
                "{} error: {}",
                self.executable,
                String::from_utf8_lossy(output.stderr.as_slice()).to_string()
            );
        }

        let stdout = String::from_utf8_lossy(output.stdout.as_slice());
        let long = stdout
            .lines()
            .nth(1)
            .ok_or_else(|| {
                anyhow::anyhow!("{} version parsing: not enough lines", self.executable)
            })?
            .split(' ')
            .nth(1)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} version parsing: not enough words in the 2nd line",
                    self.executable
                )
            })?
            .to_owned();
        let default: semver::Version = long
            .split('+')
            .next()
            .ok_or_else(|| {
                anyhow::anyhow!("{} version parsing: metadata dropping", self.executable)
            })?
            .parse()
            .map_err(|error| anyhow::anyhow!("{} version parsing: {}", self.executable, error))?;

        let l2_revision: Option<semver::Version> = stdout
            .lines()
            .nth(2)
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|line| line.split('-').nth(1))
            .and_then(|version| version.parse().ok());

        let version = Version::new(long, default, l2_revision);
        if version.default < super::FIRST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`solc` versions <{} are not supported, found {}",
                super::FIRST_SUPPORTED_VERSION,
                version.default
            );
        }
        if version.default > super::LAST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`solc` versions >{} are not supported, found {}",
                super::LAST_SUPPORTED_VERSION,
                version.default
            );
        }

        self.version = Some(version.clone());

        Ok(version)
    }
}
