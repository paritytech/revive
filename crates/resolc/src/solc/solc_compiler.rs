//! The Solidity compiler solc interface.

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use revive_common::deserialize_from_slice;
use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::CombinedJsonSelector;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputError;

use crate::solc::version::Version;

use super::Compiler;

/// The Solidity compiler.
pub struct SolcCompiler {
    /// The binary executable name.
    pub executable: String,
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
        Ok(Self { executable })
    }
}

impl Compiler for SolcCompiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &self,
        input: &mut SolcStandardJsonInput,
        messages: &mut Vec<SolcStandardJsonOutputError>,
        base_path: Option<String>,
        include_paths: Vec<String>,
        allow_paths: Option<String>,
    ) -> anyhow::Result<SolcStandardJsonOutput> {
        let mut command = std::process::Command::new(self.executable.as_str());
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.arg("--standard-json");

        for include_path in include_paths.into_iter() {
            command.arg("--include-path");
            command.arg(include_path);
        }
        if let Some(base_path) = base_path {
            command.arg("--base-path");
            command.arg(base_path);
        }
        if let Some(allow_paths) = allow_paths {
            command.arg("--allow-paths");
            command.arg(allow_paths);
        }

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

        let mut output: SolcStandardJsonOutput = deserialize_from_slice(output.stdout.as_slice())
            .map_err(|error| {
            anyhow::anyhow!(
                "{} subprocess output parsing error: {}\n{}",
                self.executable,
                error,
                deserialize_from_slice::<serde_json::Value>(output.stdout.as_slice())
                    .map(|json| serde_json::to_string_pretty(&json).expect("Always valid"))
                    .unwrap_or_else(
                        |_| String::from_utf8_lossy(output.stdout.as_slice()).to_string()
                    ),
            )
        })?;
        output
            .errors
            .retain(|error| match error.error_code.as_deref() {
                Some(code) => !SolcStandardJsonOutputError::IGNORED_WARNING_CODES.contains(&code),
                None => true,
            });
        output.errors.append(messages);

        let mut suppressed_warnings = input.suppressed_warnings.clone();
        suppressed_warnings.extend_from_slice(input.settings.suppressed_warnings.as_slice());

        input.resolve_sources();
        output.preprocess_ast(&input.sources, &suppressed_warnings)?;

        Ok(output)
    }

    /// The `solc --combined-json abi,hashes...` mirror.
    fn combined_json(
        &self,
        paths: &[PathBuf],
        mut selectors: HashSet<CombinedJsonSelector>,
    ) -> anyhow::Result<CombinedJson> {
        selectors.retain(|selector| selector.is_source_solc());
        if selectors.is_empty() {
            let version = &self.version()?.default;
            return Ok(CombinedJson::new(version.to_owned(), None));
        }

        let executable = self.executable.to_owned();

        let mut command = std::process::Command::new(executable.as_str());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.args(paths);
        command.arg("--combined-json");
        command.arg(
            selectors
                .into_iter()
                .map(|selector| selector.to_string())
                .collect::<Vec<String>>()
                .join(","),
        );

        let process = command
            .spawn()
            .map_err(|error| anyhow::anyhow!("{executable} subprocess spawning: {error:?}"))?;

        let result = process.wait_with_output().map_err(|error| {
            anyhow::anyhow!("{} subprocess output reading: {error:?}", self.executable)
        })?;

        if !result.status.success() {
            anyhow::bail!(
                "{} subprocess failed with exit code {:?}:\n{}\n{}",
                self.executable,
                result.status.code(),
                String::from_utf8_lossy(result.stdout.as_slice()),
                String::from_utf8_lossy(result.stderr.as_slice()),
            );
        }

        deserialize_from_slice::<CombinedJson>(result.stdout.as_slice()).map_err(|error| {
            anyhow::anyhow!(
                "{} subprocess stdout parsing: {error:?} (stderr: {})",
                self.executable,
                String::from_utf8_lossy(result.stderr.as_slice()),
            )
        })
    }

    /// The `solc --version` mini-parser.
    fn version(&self) -> anyhow::Result<Version> {
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

        Version::new(long, default).validate()
    }
}
