//! The Solidity compiler.

pub mod combined_json;
pub mod pipeline;
#[cfg(not(target_os = "emscripten"))]
pub mod solc;
#[cfg(target_os = "emscripten")]
pub mod soljson;
pub mod standard_json;
pub mod version;

use std::path::Path;
use std::path::PathBuf;

use self::combined_json::CombinedJson;
use self::pipeline::Pipeline;
use self::standard_json::input::Input as StandardJsonInput;
use self::standard_json::output::Output as StandardJsonOutput;
use self::version::Version;

/// The first version of `solc` with the support of standard JSON interface.
const FIRST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 4, 12);

/// The first version of `solc`, where Yul codegen is considered robust enough.
pub(crate) const FIRST_YUL_VERSION: semver::Version = semver::Version::new(0, 8, 0);

/// The first version of `solc`, where `--via-ir` codegen mode is supported.
const FIRST_VIA_IR_VERSION: semver::Version = semver::Version::new(0, 8, 13);

/// The last supported version of `solc`.
pub(crate) const LAST_SUPPORTED_VERSION: semver::Version = semver::Version::new(0, 8, 26);

/// The Solidity compiler.
pub trait Compiler {
    /// Compiles the Solidity `--standard-json` input into Yul IR.
    fn standard_json(
        &mut self,
        input: StandardJsonInput,
        pipeline: Pipeline,
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
