//! The Solidity compiler.

use std::collections::HashSet;
use std::path::PathBuf;

use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::CombinedJsonSelector;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;
use revive_solc_json_interface::SolcStandardJsonInputSettingsSelection;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputError;

use self::version::Version;

#[cfg(not(target_os = "emscripten"))]
pub mod solc_compiler;
#[cfg(target_os = "emscripten")]
pub mod soljson_compiler;
pub mod version;

/// The first version of `solc` with the support of standard JSON interface.
pub const FIRST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 0);

/// The last supported version of `solc`.
pub const LAST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 33);

/// The Solidity compiler.
pub trait Compiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &self,
        input: &mut SolcStandardJsonInput,
        messages: &mut Vec<SolcStandardJsonOutputError>,
        base_path: Option<String>,
        include_paths: Vec<String>,
        allow_paths: Option<String>,
    ) -> anyhow::Result<SolcStandardJsonOutput>;

    /// The `solc --combined-json abi,hashes...` mirror.
    fn combined_json(
        &self,
        paths: &[PathBuf],
        selectors: HashSet<CombinedJsonSelector>,
    ) -> anyhow::Result<CombinedJson>;

    /// Validates the Yul project as paths and libraries.
    fn validate_yul_paths(
        &self,
        paths: &[PathBuf],
        libraries: SolcStandardJsonInputSettingsLibraries,
        messages: &mut Vec<SolcStandardJsonOutputError>,
    ) -> anyhow::Result<SolcStandardJsonOutput> {
        let mut solc_input =
            SolcStandardJsonInput::from_yul_paths(paths, libraries, Default::default(), vec![]);
        self.validate_yul_standard_json(&mut solc_input, messages)
    }

    /// Validates the Yul project as standard JSON input.
    fn validate_yul_standard_json(
        &self,
        solc_input: &mut SolcStandardJsonInput,
        messages: &mut Vec<SolcStandardJsonOutputError>,
    ) -> anyhow::Result<SolcStandardJsonOutput> {
        solc_input.extend_selection(SolcStandardJsonInputSettingsSelection::new_yul_validation());
        let solc_output = self.standard_json(solc_input, messages, None, vec![], None)?;
        Ok(solc_output)
    }

    /// The `solc --version` mini-parser.
    fn version(&self) -> anyhow::Result<Version>;
}
