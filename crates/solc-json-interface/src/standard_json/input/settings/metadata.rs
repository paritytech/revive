//! The `solc --standard-json` input settings metadata.

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::metadata_hash::MetadataHash;

/// The `solc --standard-json` input settings metadata.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// The bytecode hash mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode_hash: Option<MetadataHash>,
}

impl Metadata {
    /// A shortcut constructor.
    pub fn new(bytecode_hash: MetadataHash) -> Self {
        Self {
            bytecode_hash: Some(bytecode_hash),
        }
    }
}
