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
}

impl Version {
    /// A shortcut constructor.
    pub fn new(long: String, default: semver::Version) -> Self {
        Self { long, default }
    }

    /// Returns an error if an unsupported version is detected.
    pub fn validate(self) -> anyhow::Result<Self> {
        if self.default < super::FIRST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`solc` versions <{} are not supported, found {}",
                super::FIRST_SUPPORTED_VERSION,
                self.default
            );
        }
        if self.default > super::LAST_SUPPORTED_VERSION {
            anyhow::bail!(
                "`solc` versions >{} are not supported, found {}",
                super::LAST_SUPPORTED_VERSION,
                self.default
            );
        }
        Ok(self)
    }
}
