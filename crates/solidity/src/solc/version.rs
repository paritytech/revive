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
    pub l2_revision: Option<semver::Version>,
}

impl Version {
    /// A shortcut constructor.
    pub fn new(
        long: String,
        default: semver::Version,
        l2_revision: Option<semver::Version>,
    ) -> Self {
        Self {
            long,
            default,
            l2_revision,
        }
    }

    /// A shortcut constructor for a simple version.
    pub fn new_simple(version: semver::Version) -> Self {
        Self {
            long: version.to_string(),
            default: version,
            l2_revision: None,
        }
    }
}
