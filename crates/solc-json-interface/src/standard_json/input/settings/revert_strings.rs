//! The revert strings mode.

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
    /// Also extend the user-supplied revert strings, which solc does not yet implement.
    #[serde(rename = "verboseDebug")]
    VerboseDebug,
}
