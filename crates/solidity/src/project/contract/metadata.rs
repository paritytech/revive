//! The Solidity contract metadata.

use serde::Serialize;

use crate::ResolcVersion;

/// The Solidity contract metadata.
/// Is used to append the metadata hash to the contract bytecode.
#[derive(Debug, Serialize)]
pub struct Metadata {
    /// The `solc` metadata.
    pub solc_metadata: serde_json::Value,
    /// The `solc` version.
    pub solc_version: semver::Version,
    /// The pallet revive edition.
    pub revive_pallet_version: Option<semver::Version>,
    /// The PolkaVM compiler version.
    pub revive_version: String,
    /// The PolkaVM compiler optimizer settings.
    pub optimizer_settings: revive_llvm_context::OptimizerSettings,
}

impl Metadata {
    /// A shortcut constructor.
    pub fn new(
        solc_metadata: serde_json::Value,
        solc_version: semver::Version,
        revive_pallet_version: Option<semver::Version>,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
    ) -> Self {
        Self {
            solc_metadata,
            solc_version,
            revive_pallet_version,
            revive_version: ResolcVersion::default().long,
            optimizer_settings,
        }
    }
}
