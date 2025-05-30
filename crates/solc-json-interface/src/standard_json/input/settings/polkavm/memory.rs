//! The compile time PolkaVM memory configuration settings.

use serde::{Deserialize, Serialize};

pub const DEFAULT_HEAP_SIZE: u32 = 64 * 1024;
pub const DEFAULT_STACK_SIZE: u32 = 32 * 1024;

/// The PolkaVM memory configuration.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// The emulated EVM linear heap memory size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heap_size: Option<u32>,
    /// The PVM stack size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<u32>,
}

impl MemoryConfig {
    /// A shorthand constructor.
    pub fn new(heap_size: Option<u32>, stack_size: Option<u32>) -> Self {
        Self {
            heap_size,
            stack_size,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            heap_size: Some(DEFAULT_HEAP_SIZE),
            stack_size: Some(DEFAULT_STACK_SIZE),
        }
    }
}
