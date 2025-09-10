//! The `solc --standard-json` output.

pub mod contract;
pub mod error;
pub mod source;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::warning::Warning;
use crate::standard_json::output::error::error_handler::ErrorHandler;
#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSource;

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
    pub fn preprocess_ast(
        &mut self,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
        suppressed_warnings: &[Warning],
    ) -> anyhow::Result<()> {
        #[cfg(feature = "parallel")]
        use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

        let id_paths: BTreeMap<usize, &String> = self
            .sources
            .iter()
            .map(|(path, source)| (source.id, path))
            .collect();

        #[cfg(feature = "parallel")]
        let iter = self.sources.par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = self.sources.iter();

        let messages: Vec<SolcStandardJsonOutputError> = iter
            .flat_map(|(_path, source)| {
                source
                    .ast
                    .as_ref()
                    .map(|ast| Source::get_messages(ast, &id_paths, sources, suppressed_warnings))
                    .unwrap_or_default()
            })
            .collect();
        self.errors.extend(messages);

        Ok(())
    }

    /// Pushes an arbitrary error with path.
    ///
    /// Please do not push project-general errors without paths here.
    pub fn push_error(&mut self, path: Option<String>, error: anyhow::Error) {
        use crate::standard_json::output::error::source_location::SourceLocation;

        self.errors.push(SolcStandardJsonOutputError::new_error(
            error,
            path.map(SourceLocation::new),
            None,
        ));
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
