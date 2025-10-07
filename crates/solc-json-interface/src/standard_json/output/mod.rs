//! The `solc --standard-json` output.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::warning::Warning;
use crate::standard_json::output::error::error_handler::ErrorHandler;
#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSettingsSelection;
#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSource;
#[cfg(all(feature = "parallel", feature = "resolc"))]
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use self::contract::Contract;
use self::error::Error as SolcStandardJsonOutputError;
use self::source::Source;

pub mod contract;
pub mod error;
pub mod source;

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
    /// Initializes a standard JSON output.
    ///
    /// Is used for projects compiled without `solc`.
    pub fn new(
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
        messages: &mut Vec<SolcStandardJsonOutputError>,
    ) -> Self {
        let sources = sources
            .iter()
            .enumerate()
            .map(|(index, (path, source))| {
                (
                    path.to_owned(),
                    Source {
                        id: index,
                        ast: source.content().map(|x| serde_json::to_value(x).unwrap()),
                    },
                )
            })
            .collect::<BTreeMap<String, Source>>();

        Self {
            contracts: BTreeMap::new(),
            sources: sources.clone(),
            errors: std::mem::take(messages),

            version: None,
            long_version: None,
            revive_version: None,
        }
    }

    /// Initializes a standard JSON output with messages.
    ///
    /// Is used to emit errors in standard JSON mode.
    pub fn new_with_messages(messages: Vec<SolcStandardJsonOutputError>) -> Self {
        Self {
            contracts: BTreeMap::new(),
            sources: BTreeMap::new(),
            errors: messages,

            version: None,
            long_version: None,
            revive_version: None,
        }
    }

    /// Prunes the output JSON and prints it to stdout.
    pub fn write_and_exit(
        mut self,
        selection_to_prune: SolcStandardJsonInputSettingsSelection,
    ) -> ! {
        for (path, contracts) in self.contracts.iter_mut() {
            for (_, contract) in contracts {
                if selection_to_prune.contains(
                    path,
                    &crate::SolcStandardJsonInputSettingsSelectionFileFlag::Metadata,
                ) {
                    contract.metadata = serde_json::Value::Null;
                }
                if selection_to_prune.contains(
                    path,
                    &crate::SolcStandardJsonInputSettingsSelectionFileFlag::Yul,
                ) {
                    contract.ir_optimized = String::new();
                }
                if let Some(ref mut evm) = contract.evm {
                    if selection_to_prune.contains(
                        path,
                        &crate::SolcStandardJsonInputSettingsSelectionFileFlag::MethodIdentifiers,
                    ) {
                        evm.method_identifiers.clear();
                    }
                }
            }
        }

        self.contracts.retain(|_, contracts| {
            contracts.retain(|_, contract| !contract.is_empty());
            !contracts.is_empty()
        });

        serde_json::to_writer(std::io::stdout(), &self).expect("Stdout writing error");
        std::process::exit(revive_common::EXIT_CODE_SUCCESS);
    }

    /// Traverses the AST and returns the list of additional errors and warnings.
    pub fn preprocess_ast(
        &mut self,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
        suppressed_warnings: &[Warning],
    ) -> anyhow::Result<()> {
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
            .filter(|error| error.is_error())
            .collect()
    }

    fn take_warnings(&mut self) -> Vec<SolcStandardJsonOutputError> {
        let warnings = self
            .errors
            .iter()
            .filter(|message| message.is_warning())
            .cloned()
            .collect();
        self.errors.retain(|message| !message.is_warning());
        warnings
    }
}
