//! Process for compiling a single compilation unit.
//! The input data.

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
    pub debug_config: Option<revive_llvm_context::DebugConfig>,
}

impl Input {
    /// A shortcut constructor.
    pub fn new(
        contract: Contract,
        project: Project,
        include_metadata_hash: bool,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        debug_config: Option<revive_llvm_context::DebugConfig>,
    ) -> Self {
        Self {
            contract,
            project,
            include_metadata_hash,
            optimizer_settings,
            debug_config,
        }
    }
}
