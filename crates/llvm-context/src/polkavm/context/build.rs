//! The LLVM module build.

use revive_common::BYTE_LENGTH_WORD;
use serde::Deserialize;
use serde::Serialize;

/// The LLVM module build.
#[derive(Debug, Serialize, Deserialize)]
pub struct Build {
    /// The PolkaVM text assembly.
    pub assembly_text: Option<String>,
    /// The metadata hash.
    pub metadata_hash: Option<[u8; BYTE_LENGTH_WORD]>,
    /// The PolkaVM binary bytecode.
    pub bytecode: Vec<u8>,
    /// The PolkaVM bytecode hash. Unlinked builds don't have a hash yet.
    pub bytecode_hash: Option<[u8; BYTE_LENGTH_WORD]>,
}

impl Build {
    /// A shortcut constructor.
    pub fn new(metadata_hash: Option<[u8; BYTE_LENGTH_WORD]>, bytecode: Vec<u8>) -> Self {
        Self {
            assembly_text: None,
            metadata_hash,
            bytecode,
            bytecode_hash: None,
        }
    }
}
