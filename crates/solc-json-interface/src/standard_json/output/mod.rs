//! The `solc --standard-json` output.

pub mod contract;
pub mod error;
pub mod source;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::warning::Warning;

use self::contract::Contract;
use self::error::Error as SolcStandardJsonOutputError;
use self::source::Source;

/// The `solc --standard-json` output.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Output {
    /// The file-contract hashmap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contracts: Option<BTreeMap<String, BTreeMap<String, Contract>>>,
    /// The source code mapping data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<BTreeMap<String, Source>>,
    /// The compilation errors and warnings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<SolcStandardJsonOutputError>>,
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

impl Output {
    /// Traverses the AST and returns the list of additional errors and warnings.
    #[cfg(feature = "resolc")]
    pub fn preprocess_ast(&mut self, suppressed_warnings: &[Warning]) -> anyhow::Result<()> {
        let sources = match self.sources.as_ref() {
            Some(sources) => sources,
            None => return Ok(()),
        };

        let mut messages = Vec::new();
        for (path, source) in sources.iter() {
            if let Some(ast) = source.ast.as_ref() {
                let mut polkavm_messages = Source::get_messages(ast, suppressed_warnings);
                for message in polkavm_messages.iter_mut() {
                    message.push_contract_path(path.as_str());
                }
                messages.extend(polkavm_messages);
            }
        }
        self.errors = match self.errors.take() {
            Some(mut errors) => {
                errors.extend(messages);
                Some(errors)
            }
            None => Some(messages),
        };

        Ok(())
    }
}
