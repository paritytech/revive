//! The `solc --standard-json` input settings YUL optimizer details.

use serde::Deserialize;
use serde::Serialize;

/// The `solc --standard-json` input settings optimizer YUL details.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YulDetails {
    /// Whether the stack allocation pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_allocation: Option<bool>,
    /// The optimization step sequence string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_steps: Option<String>,
}

impl YulDetails {
    /// A shortcut constructor.
    pub fn new(stack_allocation: Option<bool>, optimizer_steps: Option<String>) -> Self {
        Self {
            stack_allocation,
            optimizer_steps,
        }
    }
}
