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
}

impl Optimizer {
    /// A shortcut constructor.
    pub fn new(enabled: bool, mode: char, details: Details) -> Self {
        Self {
            enabled,
            mode,
            details,
        }
    }

    /// The default optimization mode.
    pub fn default_mode() -> char {
        'z'
    }

    /// Optimizer settings tuned for PolkaVM code size.
    ///
    /// Uses the default solc optimizer with a custom Yul sequence that adds
    /// an extra `[LScsTulD]` cleanup loop for better code size on PolkaVM.
    pub fn for_polkavm(enabled: bool) -> Self {
        let details = if enabled {
            Details::for_polkavm()
        } else {
            Details::default()
        };
        Self::new(enabled, Self::default_mode(), details)
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new(true, Self::default_mode(), Details::default())
    }
}
