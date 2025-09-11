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
    pub solc_version: Option<semver::Version>,
    /// The pallet revive edition.
    pub revive_pallet_version: Option<semver::Version>,
    /// The PolkaVM compiler version.
    pub revive_version: String,
    /// The PolkaVM compiler optimizer settings.
    pub optimizer_settings: revive_llvm_context::OptimizerSettings,
    /// The extra LLVM arguments give used for manual control.
    pub llvm_arguments: Vec<String>,
}

impl Metadata {
    /// A shortcut constructor.
    pub fn new(
        solc_metadata: serde_json::Value,
        solc_version: Option<semver::Version>,
        revive_pallet_version: Option<semver::Version>,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        llvm_arguments: Vec<String>,
    ) -> Self {
        Self {
            solc_metadata,
            solc_version,
            revive_pallet_version,
            revive_version: ResolcVersion::default().long,
            optimizer_settings,
            llvm_arguments,
        }
    }
}
