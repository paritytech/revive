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
    /// The LLVM version string.
    pub llvm: semver::Version,
}

impl Default for Version {
    fn default() -> Self {
        let default = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Always valid");
        let commit = env!("GIT_COMMIT_HASH");
        let (llvm_major, llvm_minor, llvm_patch) = inkwell::support::get_llvm_version();
        let llvm = semver::Version::new(llvm_major as u64, llvm_minor as u64, llvm_patch as u64);

        Self {
            long: format!("{default}+commit.{commit}.llvm-{llvm}"),
            default,
            llvm,
        }
    }
}
