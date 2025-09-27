//! Process for compiling a single compilation unit.
//! The input data.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use revive_common::MetadataHash;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use serde::Deserialize;
use serde::Serialize;

use crate::project::contract::Contract;
use crate::SolcVersion;

/// The input data.
#[derive(Debug, Serialize, Deserialize)]
pub struct Input {
    /// The contract representation.
    pub contract: Contract,
    /// The `solc` compiler version.
    pub solc_version: Option<SolcVersion>,
    /// Whether to append the metadata hash.
    pub metadata_hash: MetadataHash,
    /// The optimizer settings.
    pub optimizer_settings: revive_llvm_context::OptimizerSettings,
    /// The debug output config.
    pub debug_config: revive_llvm_context::DebugConfig,
    /// The extra LLVM arguments give used for manual control.
    pub llvm_arguments: Vec<String>,
    /// The PVM memory configuration.
    pub memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    /// Missing unlinked libraries.
    pub missing_libraries: BTreeSet<String>,
    /// Factory dependencies.
    pub factory_dependencies: BTreeSet<String>,
    /// The mapping of auxiliary identifiers, e.g. Yul object names, to full contract paths.
    pub identifier_paths: BTreeMap<String, String>,
}

impl Input {
    /// A shortcut constructor.
    pub fn new(
        contract: Contract,
        solc_version: Option<SolcVersion>,
        metadata_hash: MetadataHash,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: Vec<String>,
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
        missing_libraries: BTreeSet<String>,
        factory_dependencies: BTreeSet<String>,
        identifier_paths: BTreeMap<String, String>,
    ) -> Self {
        Self {
            contract,
            solc_version,
            metadata_hash,
            optimizer_settings,
            debug_config,
            llvm_arguments,
            memory_config,
            missing_libraries,
            factory_dependencies,
            identifier_paths,
        }
    }
}
