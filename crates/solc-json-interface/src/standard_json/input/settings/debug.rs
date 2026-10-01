//! The `solc --standard-json` input debugging settings.

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::revert_strings::RevertStrings;

/// The `solc --standard-json` input debugging settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Debug {
    /// How to treat revert and require reason strings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert_strings: Option<RevertStrings>,
    /// The debug information components that solc adds as comments to its Yul output.
    /// Kept as strings because the available components depend on the solc version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<Vec<String>>,
}

impl Debug {
    /// A shortcut constructor.
    pub fn new(revert_strings: Option<RevertStrings>, debug_info: Option<Vec<String>>) -> Self {
        Self {
            revert_strings,
            debug_info,
        }
    }
}
