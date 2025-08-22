//! The revive binary object helper module.

use std::str::FromStr;

/// The binary object format.
///
/// Unlinked contracts are stored in a different object format
/// than final (linked) contract blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ObjectFormat {
    /// The unlinked ELF object.
    ELF,

    /// The fully linked PVM blob.
    PVM,
}

impl FromStr for ObjectFormat {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ELF" => Ok(Self::ELF),
            "PVM" => Ok(Self::PVM),
            _ => anyhow::bail!(
                "Unknown object format: {value}. Supported formats: {}, {}",
                Self::ELF.to_string(),
                Self::PVM.to_string()
            ),
        }
    }
}

impl std::fmt::Display for ObjectFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ELF => write!(f, "ELF"),
            Self::PVM => write!(f, "PVM"),
        }
    }
}
