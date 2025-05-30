//! The `solc --standard-json` input settings optimizer.

pub mod details;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<char>,
    /// The `solc` optimizer details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
    /// Whether to try to recompile with -Oz if the bytecode is too large.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_to_optimizing_for_size: Option<bool>,
}

impl Optimizer {
    /// A shortcut constructor.
    pub fn new(
        enabled: bool,
        mode: Option<char>,
        version: &semver::Version,
        fallback_to_optimizing_for_size: bool,
    ) -> Self {
        Self {
            enabled,
            mode,
            details: Some(Details::disabled(version)),
            fallback_to_optimizing_for_size: Some(fallback_to_optimizing_for_size),
        }
    }

    /// Sets the necessary defaults.
    pub fn normalize(&mut self, version: &semver::Version) {
        self.mode = None;
        self.fallback_to_optimizing_for_size = None;
        self.details = if version >= &semver::Version::new(0, 5, 5) {
            Some(Details::disabled(version))
        } else {
            None
        };
    }
}
