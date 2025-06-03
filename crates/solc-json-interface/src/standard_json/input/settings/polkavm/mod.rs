//! The `resolc --standard-json` polkavm settings.
//!
//! Used for options specific to PolkaVM which therefor don't exist in solc.

use memory::MemoryConfig;
use serde::{Deserialize, Serialize};

pub mod memory;

/// PVM specific compiler settings.
#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize)]
pub struct PolkaVM {
    /// The PolkaVM target machine memory configuration settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_config: Option<MemoryConfig>,
    /// Instruct LLVM to emit debug information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_information: Option<bool>,
}

impl PolkaVM {
    pub fn new(memory_config: Option<MemoryConfig>, debug_information: bool) -> Self {
        Self {
            memory_config,
            debug_information: Some(debug_information),
        }
    }
}
