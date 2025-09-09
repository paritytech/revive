//! The `solc --standard-json` input.

pub mod language;
pub mod settings;
pub mod source;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[cfg(all(feature = "parallel", feature = "resolc"))]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::metadata::Metadata as SolcStandardJsonInputSettingsMetadata;
use crate::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
use crate::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
#[cfg(feature = "resolc")]
use crate::warning::Warning;
use crate::SolcStandardJsonInputSettingsLibraries;
use crate::SolcStandardJsonInputSettingsPolkaVM;

use self::language::Language;
use self::settings::Settings;
use self::source::Source;

/// The `solc --standard-json` input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    /// The input language.
    pub language: Language,
    /// The input source code files hashmap.
    pub sources: BTreeMap<String, Source>,
    /// The compiler settings.
    pub settings: Settings,
    /// The suppressed warnings.
    #[cfg(feature = "resolc")]
    #[serde(skip_serializing)]
    pub suppressed_warnings: Option<Vec<Warning>>,
}

impl Input {
    /// A shortcut constructor from stdin.
    pub fn try_from_stdin() -> anyhow::Result<Self> {
        let mut input: Self = serde_json::from_reader(std::io::BufReader::new(std::io::stdin()))?;
        input
            .settings
            .output_selection
            .get_or_insert_with(SolcStandardJsonInputSettingsSelection::default)
            .extend_with_required();
        Ok(input)
    }

    /// A shortcut constructor from paths.
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_paths(
        language: Language,
        evm_version: Option<revive_common::EVMVersion>,
        paths: &[PathBuf],
        libraries: &[String],
        remappings: Option<BTreeSet<String>>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: Option<SolcStandardJsonInputSettingsMetadata>,
        #[cfg(feature = "resolc")] suppressed_warnings: Option<Vec<Warning>>,
        polkavm: Option<SolcStandardJsonInputSettingsPolkaVM>,
    ) -> anyhow::Result<Self> {
        let mut paths: BTreeSet<PathBuf> = paths.iter().cloned().collect();
        let libraries = SolcStandardJsonInputSettingsLibraries::try_from(libraries)?;
        for library_file in libraries.as_inner().keys() {
            paths.insert(PathBuf::from(library_file));
        }

        let sources = paths
            .iter()
            .map(|path| {
                let source = Source::try_from(path.as_path()).unwrap_or_else(|error| {
                    panic!("Source code file {path:?} reading error: {error}")
                });
                (path.to_string_lossy().to_string(), source)
            })
            .collect();

        Ok(Self {
            language,
            sources,
            settings: Settings::new(
                evm_version,
                libraries,
                remappings,
                output_selection,
                optimizer,
                metadata,
                polkavm,
            ),
            #[cfg(feature = "resolc")]
            suppressed_warnings,
        })
    }

    /// A shortcut constructor from source code.
    /// Only for the integration test purposes.
    #[cfg(feature = "resolc")]
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_sources(
        evm_version: Option<revive_common::EVMVersion>,
        sources: BTreeMap<String, String>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        remappings: Option<BTreeSet<String>>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: Option<SolcStandardJsonInputSettingsMetadata>,
        suppressed_warnings: Option<Vec<Warning>>,
        polkavm: Option<SolcStandardJsonInputSettingsPolkaVM>,
    ) -> anyhow::Result<Self> {
        #[cfg(feature = "parallel")]
        let iter = sources.into_par_iter(); // Parallel iterator

        #[cfg(not(feature = "parallel"))]
        let iter = sources.into_iter(); // Sequential iterator
        let sources = iter
            .map(|(path, content)| (path, Source::from(content)))
            .collect();

        Ok(Self {
            language: Language::Solidity,
            sources,
            settings: Settings::new(
                evm_version,
                libraries,
                remappings,
                output_selection,
                optimizer,
                metadata,
                polkavm,
            ),
            suppressed_warnings,
        })
    }

    /// Sets the necessary defaults.
    pub fn normalize(&mut self) {
        self.settings.normalize();
    }
R
