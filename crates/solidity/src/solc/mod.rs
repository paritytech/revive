//! The Solidity compiler.

pub mod combined_json;
#[cfg(not(target_os = "emscripten"))]
pub mod solc_compiler;
#[cfg(target_os = "emscripten")]
pub mod soljson_compiler;
pub mod standard_json;
pub mod version;

use std::path::Path;
use std::path::PathBuf;

use self::combined_json::CombinedJson;
use self::standard_json::input::Input as StandardJsonInput;
use self::standard_json::output::Output as StandardJsonOutput;
use self::version::Version;

/// The first version of `solc` with the support of standard JSON interface.
pub const FIRST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 0);

/// The first version of `solc`, where `--via-ir` codegen mode is supported.
pub const FIRST_VIA_IR_VERSION: semver::Version = semver::Version::new(0, 8, 13);

/// The last supported version of `solc`.
pub const LAST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 28);

/// `--include-path` was introduced in solc `0.8.8` <https://github.com/ethereum/solidity/releases/tag/v0.8.8>
pub const FIRST_INCLUDE_PATH_VERSION: semver::Version = semver::Version::new(0, 8, 8);

/// The Solidity compiler.
pub trait Compiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &mut self,
        input: StandardJsonInput,
        base_path: Option<String>,
        include_paths: Vec<String>,
        allow_paths: Option<String>,
    ) -> anyhow::Result<StandardJsonOutput>;

    /// The `solc --combined-json abi,hashes...` mirror.
    fn combined_json(
        &self,
        paths: &[PathBuf],
        combined_json_argument: &str,
    ) -> anyhow::Result<CombinedJson>;

    /// The `solc` Yul validator.
    fn validate_yul(&self, path: &Path) -> anyhow::Result<()>;

    /// The `solc --version` mini-parser.
    fn version(&mut self) -> anyhow::Result<Version>;
}
