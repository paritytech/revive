//! The resolc compiler version.

use serde::Deserialize;
use serde::Serialize;

/// The resolc compiler version.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Version {
    /// The long version string.
    pub long: String,
    /// The short `semver`.
    pub default: semver::Version,
}

impl Default for Version {
    fn default() -> Self {
        let default = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Always valid");
        let commit = env!("GIT_COMMIT_HASH");

        Self {
            long: format!("{default}+commit.{commit}"),
            default,
        }
    }
}
