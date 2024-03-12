//!
//! The `solc --standard-json` input settings metadata.
//!

use serde::Deserialize;
use serde::Serialize;

///
/// The `solc --standard-json` input settings metadata.
///
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// The bytecode hash mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode_hash: Option<era_compiler_llvm_context::EraVMMetadataHash>,
}

impl Metadata {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(bytecode_hash: era_compiler_llvm_context::EraVMMetadataHash) -> Self {
        Self {
            bytecode_hash: Some(bytecode_hash),
        }
    }
}
