//! The compile time PolkaVM memory configuration settings.

use serde::{Deserialize, Serialize};

/// The PolkaVM memory configuration.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// The emulated EVM linear heap memory size in bytes.
    pub heap_size: u32,
    /// The PVM stack size in bytes.
    pub stack_size: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            heap_size: 64 * 1024,
            stack_size: 32 * 1024,
        }
    }
}
