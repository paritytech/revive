//! The Solidity compiler version.

use serde::Deserialize;
use serde::Serialize;

/// The Solidity compiler version.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Version {
    /// The long version string.
    pub long: String,
    /// The short `semver`.
    pub default: semver::Version,
    /// The L2 revision additional versioning.
    pub l2_revision: semver::Version,
}

impl Version {
    /// A shortcut constructor.
    pub fn new(long: String, default: semver::Version, l2_revision: semver::Version) -> Self {
        Self {
            long,
            default,
            l2_revision,
        }
    }

    // pub fn validate(self, include_paths: &[String]) -> anyhow::Result<Self> {
    //     if self.default < super::FIRST_SUPPORTED_VERSION {
    //         anyhow::bail!(
    //             "`solc` versions <{} are not supported, found {}",
    //             super::FIRST_SUPPORTED_VERSION,
    //             self.default
    //         );
    //     }
    //     if self.default > super::LAST_SUPPORTED_VERSION {
    //         anyhow::bail!(
    //             "`solc` versions >{} are not supported, found {}",
    //             super::LAST_SUPPORTED_VERSION,
    //             self.default
    //         );
    //     }
    //     if !include_paths.is_empty() && self.default < super::FIRST_INCLUDE_PATH_VERSION {
    //         anyhow::bail!("--include-path is not supported in solc {}", self.default);
    //     }

    //     Ok(self)
    // }
}
