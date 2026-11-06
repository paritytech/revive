//! The revert strings mode.

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

/// The revert strings mode.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RevertStrings {
    /// Keep the user-supplied revert strings without adding compiler-generated ones.
    #[serde(rename = "default")]
    Default,
    /// Remove the revert strings where possible (i.e. if literals are used),
    /// keeping their side effects. This does not remove custom errors.
    #[serde(rename = "strip")]
    Strip,
    /// Add revert strings to the compiler-generated internal reverts.
    #[serde(rename = "debug")]
    Debug,
}

impl FromStr for RevertStrings {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "default" => Ok(Self::Default),
            "strip" => Ok(Self::Strip),
            "debug" => Ok(Self::Debug),
            _ => anyhow::bail!("unknown revert strings mode: `{string}`"),
        }
    }
}
