//! The `solc --standard-json` input settings optimizer.

pub mod details;
pub mod yul_details;

use serde::Deserialize;
use serde::Serialize;

use self::details::Details;

/// The `solc --standard-json` input settings optimizer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: bool,
    /// The optimization mode string.
    #[serde(default = "Optimizer::default_mode", skip_serializing)]
    pub mode: char,
    /// The `solc` optimizer details.
    #[serde(default)]
    pub details: Details,
    /// Whether to try to recompile with -Oz if the bytecode is too large.
    #[serde(default, skip_serializing)]
    pub fallback_to_optimizing_for_size: bool,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new(true, Self::default_mode(), Details::default(), false)
    }
}

impl Optimizer {
    /// A shortcut constructor.
    pub fn new(
        enabled: bool,
        mode: char,
        details: Details,
        fallback_to_optimizing_for_size: bool,
    ) -> Self {
        Self {
            enabled,
            mode,
            details,
            fallback_to_optimizing_for_size,
        }
    }

    /// The default optimization mode.
    fn default_mode() -> char {
        'z'
    }
}
