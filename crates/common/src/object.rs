//! The revive binary object helper module.

use std::str::FromStr;

/// The binary object format.
///
/// Unlinked contracts are stored in a different object format
/// than final (linked) contract blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ObjectFormat {
    /// The unlinked ELF object format.
    ELF,
    /// The fully linked PVM format.
    PVM,
}

impl ObjectFormat {
    pub const PVM_MAGIC: [u8; 4] = [b'P', b'V', b'M', b'\0'];
    pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
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

impl TryFrom<&[u8]> for ObjectFormat {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.starts_with(&Self::PVM_MAGIC) {
            return Ok(Self::PVM);
        }
        if value.starts_with(&Self::ELF_MAGIC) {
            return Ok(Self::ELF);
        }
        Err("expected a contract object")
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
