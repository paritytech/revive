//! The metadata hash type.

use std::str::FromStr;

/// The metadata hash type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MetadataHash {
    /// Do not include bytecode hash.
    #[serde(rename = "none")]
    None,
    /// Include the `ipfs` hash.
    #[serde(rename = "ipfs")]
    IPFS,
    /// Include the `keccak256`` hash.
    #[serde(rename = "keccak256")]
    Keccak256,
}

impl FromStr for MetadataHash {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "none" => Ok(Self::None),
            "ipfs" => Ok(Self::IPFS),
            "keccak256" => Ok(Self::Keccak256),
            string => anyhow::bail!("unknown bytecode hash mode: `{string}`"),
        }
    }
}

impl std::fmt::Display for MetadataHash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::IPFS => write!(f, "ipfs"),
            Self::Keccak256 => write!(f, "keccak256"),
        }
    }
}
