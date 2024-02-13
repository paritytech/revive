//!
//! The `solc --standard-json` input settings optimizer.
//!

pub mod details;

use serde::Deserialize;
use serde::Serialize;

use self::details::Details;

///
/// The `solc --standard-json` input settings optimizer.
///
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: bool,
    /// The optimization mode string.
    #[serde(skip_serializing)]
    pub mode: Option<char>,
    /// The `solc` optimizer details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
    /// Whether to try to recompile with -Oz if the bytecode is too large.
    #[serde(skip_serializing)]
    pub fallback_to_optimizing_for_size: Option<bool>,
    /// Whether to disable the system request memoization.
    #[serde(skip_serializing)]
    pub disable_system_request_memoization: Option<bool>,
}

impl Optimizer {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        enabled: bool,
        mode: Option<char>,
        version: &semver::Version,
        fallback_to_optimizing_for_size: bool,
        disable_system_request_memoization: bool,
    ) -> Self {
        Self {
            enabled,
            mode,
            details: Some(Details::disabled(version)),
            fallback_to_optimizing_for_size: Some(fallback_to_optimizing_for_size),
            disable_system_request_memoization: Some(disable_system_request_memoization),
        }
    }

    ///
    /// Sets the necessary defaults.
    ///
    pub fn normalize(&mut self, version: &semver::Version) {
        self.details = if version >= &semver::Version::new(0, 5, 5) {
            Some(Details::disabled(version))
        } else {
            None
        };
    }
}

impl TryFrom<&Optimizer> for era_compiler_llvm_context::OptimizerSettings {
    type Error = anyhow::Error;

    fn try_from(value: &Optimizer) -> Result<Self, Self::Error> {
        let mut result = match value.mode {
            Some(mode) => Self::try_from_cli(mode)?,
            None => Self::cycles(),
        };
        if value.fallback_to_optimizing_for_size.unwrap_or_default() {
            result.enable_fallback_to_size();
        }
        if value.disable_system_request_memoization.unwrap_or_default() {
            result.disable_system_request_memoization();
        }
        Ok(result)
    }
}
