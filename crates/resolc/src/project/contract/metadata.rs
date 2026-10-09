//! The Solidity contract metadata.

use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
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
    pub revive_version: String,
    /// The PolkaVM compiler optimizer settings.
    pub optimizer_settings: OptimizerSettings,
    /// The extra LLVM arguments give used for manual control.
    pub llvm_arguments: Vec<String>,
    /// The PolkaVM memory configuration.
    pub memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    /// Whether the newyork pipeline was used.
    pub newyork: bool,
    /// Whether debug information was emitted.
    pub debug_information: bool,
}

impl Metadata {
    /// A shortcut constructor.
    pub fn new(
        solc_metadata: serde_json::Value,
        solc_version: Option<semver::Version>,
        optimizer_settings: OptimizerSettings,
        llvm_arguments: Vec<String>,
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
        newyork: bool,
        debug_information: bool,
    ) -> Self {
        Self {
            solc_metadata,
            solc_version,
            revive_version: ResolcVersion::default().long,
            optimizer_settings,
            llvm_arguments,
            memory_config,
            newyork,
            debug_information,
        }
    }
}
