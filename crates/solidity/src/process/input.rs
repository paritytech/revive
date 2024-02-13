//!
//! Process for compiling a single compilation unit.
//!
//! The input data.
//!

use serde::Deserialize;
use serde::Serialize;

use crate::project::contract::Contract;
use crate::project::Project;

///
/// The input data.
///
#[derive(Debug, Serialize, Deserialize)]
pub struct Input {
    /// The contract representation.
    pub contract: Contract,
    /// The project representation.
    pub project: Project,
    /// The system mode flag.
    pub is_system_mode: bool,
    /// Whether to append the metadata hash.
    pub include_metadata_hash: bool,
    /// Enables the test bytecode encoding.
    pub enable_test_encoding: bool,
    /// The optimizer settings.
    pub optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
    /// The debug output config.
    pub debug_config: Option<era_compiler_llvm_context::DebugConfig>,
}

impl Input {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        contract: Contract,
        project: Project,
        is_system_mode: bool,
        include_metadata_hash: bool,
        enable_test_encoding: bool,
        optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
        debug_config: Option<era_compiler_llvm_context::DebugConfig>,
    ) -> Self {
        Self {
            contract,
            project,
            is_system_mode,
            include_metadata_hash,
            enable_test_encoding,
            optimizer_settings,
            debug_config,
        }
    }
}
