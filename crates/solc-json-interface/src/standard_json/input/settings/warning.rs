//! `resolc` custom compiler warnings.
//!
//! The revive compiler adds warnings only applicable when compilng
//! to the revive stack on Polkadot to the output.

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

// The `resolc` custom compiler warning.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    TxOrigin,
    AssemblyCreate,
}

impl Warning {
    /// Converts string arguments into an array of warnings.
    pub fn try_from_strings(strings: &[String]) -> Result<Vec<Self>, anyhow::Error> {
        strings
            .iter()
            .map(|string| Self::from_str(string))
            .collect()
    }
}

impl FromStr for Warning {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "txorigin" => Ok(Self::TxOrigin),
            "txorigin" => Ok(Self::AssemblyCreate),
            _ => Err(anyhow::anyhow!("Invalid warning: {}", string)),
        }
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::TxOrigin => write!(f, "txorigin"),
            Self::AssemblyCreate => write!(f, "assemblycreate"),
        }
    }
}
