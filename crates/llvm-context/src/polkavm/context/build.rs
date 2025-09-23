//! The LLVM module build.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

/// The LLVM module build.
#[derive(Debug, Serialize, Deserialize)]
pub struct Build {
    /// The PolkaVM text assembly.
    pub assembly_text: Option<String>,
    /// The metadata hash.
    pub metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
    /// The PolkaVM binary bytecode.
    pub bytecode: Vec<u8>,
    /// The PolkaVM bytecode hash. Unlinked builds don't have a hash yet.
    pub bytecode_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
    /// The hash-to-full-path mapping of the contract factory dependencies.
    pub factory_dependencies: BTreeMap<String, String>,
}

impl Build {
    /// A shortcut constructor.
    pub fn new(
        assembly_text: Option<String>,
        metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
        bytecode: Vec<u8>,
    ) -> Self {
        Self {
            assembly_text,
            metadata_hash,
            bytecode,
            bytecode_hash: None,
            factory_dependencies: BTreeMap::new(),
        }
    }
}
