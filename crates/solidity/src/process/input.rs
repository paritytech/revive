//! Process for compiling a single compilation unit.
//! The input data.

use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use serde::Deserialize;
use serde::Serialize;

use crate::project::contract::Contract;
use crate::project::Project;

/// The input data.
#[derive(Debug, Serialize, Deserialize)]
pub struct Input {
    /// The contract representation.
    pub contract: Contract,
    /// The project representation.
    pub project: Project,
    /// Whether to append the metadata hash.
    pub include_metadata_hash: bool,
    /// The optimizer settings.
    pub optimizer_settings: revive_llvm_context::OptimizerSettings,
    /// The debug output config.
    pub debug_config: revive_llvm_context::DebugConfig,
    /// The extra LLVM arguments give used for manual control.
    pub llvm_arguments: Vec<String>,
    /// The PVM memory configuration.
    pub memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
}

impl Input {
    /// A shortcut constructor.
    pub fn new(
        contract: Contract,
        project: Project,
        include_metadata_hash: bool,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: Vec<String>,
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    ) -> Self {
        Self {
            contract,
            project,
            include_metadata_hash,
            optimizer_settings,
            debug_config,
            llvm_arguments,
            memory_config,
        }
    }
}
