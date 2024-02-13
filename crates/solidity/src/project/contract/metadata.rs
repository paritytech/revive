//!
//! The Solidity contract metadata.
//!

use serde::Serialize;

///
/// The Solidity contract metadata.
///
/// Is used to append the metadata hash to the contract bytecode.
///
#[derive(Debug, Serialize)]
pub struct Metadata {
    /// The `solc` metadata.
    pub solc_metadata: serde_json::Value,
    /// The `solc` version.
    pub solc_version: semver::Version,
    /// The zkVM `solc` edition.
    pub solc_zkvm_edition: Option<semver::Version>,
    /// The EraVM compiler version.
    pub zk_version: semver::Version,
    /// The EraVM compiler optimizer settings.
    pub optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
}

impl Metadata {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        solc_metadata: serde_json::Value,
        solc_version: semver::Version,
        solc_zkvm_edition: Option<semver::Version>,
        zk_version: semver::Version,
        optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
    ) -> Self {
        Self {
            solc_metadata,
            solc_version,
            solc_zkvm_edition,
            zk_version,
            optimizer_settings,
        }
    }
}
