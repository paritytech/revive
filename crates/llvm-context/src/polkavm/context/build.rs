//! The LLVM module build.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

/// The LLVM module build.
#[derive(Debug, Serialize, Deserialize)]
pub struct Build {
    /// The PolkaVM text assembly.
    pub assembly_text: String,
    /// The metadata hash.
    pub metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_FIELD]>,
    /// The PolkaVM binary bytecode.
    pub bytecode: Vec<u8>,
    /// The PolkaVM bytecode hash.
    pub bytecode_hash: String,
    /// The hash-to-full-path mapping of the contract factory dependencies.
    pub factory_dependencies: BTreeMap<String, String>,
}

impl Build {
    /// A shortcut constructor.
    pub fn new(
        assembly_text: String,
        metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_FIELD]>,
        bytecode: Vec<u8>,
        bytecode_hash: String,
    ) -> Self {
        Self {
            assembly_text,
            metadata_hash,
            bytecode,
            bytecode_hash,
            factory_dependencies: BTreeMap::new(),
        }
    }
}
