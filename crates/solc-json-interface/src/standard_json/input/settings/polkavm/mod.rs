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
    pub memory_config: MemoryConfig,
    /// Instruct LLVM to emit debug information.
    pub debug_information: bool,
}

impl PolkaVM {
    pub fn new(memory_config: Option<MemoryConfig>, debug_information: bool) -> Self {
        Self {
            memory_config: memory_config.unwrap_or_default(),
            debug_information,
        }
    }
}
