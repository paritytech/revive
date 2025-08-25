//! The `solc --standard-json` output.

pub mod contract;
pub mod error;
pub mod source;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::output::error::error_handler::ErrorHandler;
#[cfg(feature = "resolc")]
use crate::warning::Warning;

use self::contract::Contract;
use self::error::Error as SolcStandardJsonOutputError;
use self::source::Source;

/// The `solc --standard-json` output.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Output {
    /// The file-contract hashmap.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub contracts: BTreeMap<String, BTreeMap<String, Contract>>,
    /// The source code mapping data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, Source>,
    /// The compilation errors and warnings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SolcStandardJsonOutputError>,
    /// The `solc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The `solc` compiler long version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_version: Option<String>,
    /// The `resolc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revive_version: Option<String>,
}

#[cfg(feature = "resolc")]
impl Output {
    /// Traverses the AST and returns the list of additional errors and warnings.
    pub fn preprocess_ast(&mut self, suppressed_warnings: &[Warning]) -> anyhow::Result<()> {
        let messages: Vec<SolcStandardJsonOutputError> = self
            .sources
            .iter()
            .map(|(_path, source)| {
                source
                    .ast
                    .as_ref()
                    .map(|ast| Source::get_messages(ast, suppressed_warnings))
                    .unwrap_or_default()
            })
            .flatten()
            .collect();
        self.errors.extend(messages);

        Ok(())
    }
}

impl ErrorHandler for Output {
    fn errors(&self) -> Vec<&SolcStandardJsonOutputError> {
        self.errors
            .iter()
            .filter(|error| error.severity == "error")
            .collect()
    }

    fn take_warnings(&mut self) -> Vec<SolcStandardJsonOutputError> {
        let warnings = self
            .errors
            .iter()
            .filter(|message| message.severity == "warning")
            .cloned()
            .collect();
        self.errors.retain(|message| message.severity != "warning");
        warnings
    }
}
