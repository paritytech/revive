//! The Solidity compiler.

#[cfg(not(target_os = "emscripten"))]
pub mod solc_compiler;
#[cfg(target_os = "emscripten")]
pub mod soljson_compiler;
pub mod version;

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::CombinedJsonSelector;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputError;

use self::version::Version;

/// The first version of `solc` with the support of standard JSON interface.
pub const FIRST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 0);

/// The last supported version of `solc`.
pub const LAST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 30);

/// `--include-path` was introduced in solc `0.8.8` <https://github.com/ethereum/solidity/releases/tag/v0.8.8>
pub const FIRST_INCLUDE_PATH_VERSION: semver::Version = semver::Version::new(0, 8, 8);

/// The Solidity compiler.
pub trait Compiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &mut self,
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

    /// The `solc` Yul validator.
    fn validate_yul(&self, path: &Path) -> anyhow::Result<()>;

    /// The `solc --version` mini-parser.
    fn version(&self) -> anyhow::Result<Version>;
}
