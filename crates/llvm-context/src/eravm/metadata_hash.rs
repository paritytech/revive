//! The metadata hash mode.

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

/// The metadata hash mode.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataHash {
    /// Do not include bytecode hash.
    #[serde(rename = "none")]
    None,
    /// The default keccak256 hash.
    #[serde(rename = "keccak256")]
    Keccak256,
}

impl FromStr for MetadataHash {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "none" => Ok(Self::None),
            "keccak256" => Ok(Self::Keccak256),
            _ => anyhow::bail!("Unknown bytecode hash mode: `{}`", string),
        }
    }
}
