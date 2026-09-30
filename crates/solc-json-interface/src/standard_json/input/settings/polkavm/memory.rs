//! The compile time PolkaVM memory configuration settings.

use polkavm_common::abi::{MemoryMapBuilder, VM_MAX_PAGE_SIZE};
use serde::{Deserialize, Serialize};

pub const DEFAULT_HEAP_SIZE: u32 = 128 * 1024;
pub const DEFAULT_STACK_SIZE: u32 = 128 * 1024;

/// The PolkaVM memory configuration.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
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

    /// Checks that the heap and stack sizes fit into the PVM memory map.
    pub fn validate(&self) -> anyhow::Result<()> {
        let heap_size = self.heap_size.unwrap_or(DEFAULT_HEAP_SIZE);
        let stack_size = self.stack_size.unwrap_or(DEFAULT_STACK_SIZE);
        MemoryMapBuilder::new(VM_MAX_PAGE_SIZE)
            .rw_data_size(heap_size)
            .stack_size(stack_size)
            .build()
            .map_err(|error| {
                anyhow::anyhow!(
                    "Heap size {heap_size} and stack size {stack_size} do not fit into the PVM memory map: {error}"
                )
            })?;
        Ok(())
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
