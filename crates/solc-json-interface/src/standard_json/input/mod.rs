//! The `solc --standard-json` input.

pub mod language;
pub mod settings;
pub mod source;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

#[cfg(all(feature = "parallel", feature = "resolc"))]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::metadata::Metadata as SolcStandardJsonInputSettingsMetadata;
use crate::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
use crate::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
use crate::SolcStandardJsonInputSettingsLibraries;
use crate::SolcStandardJsonInputSettingsPolkaVM;

use self::language::Language;
#[cfg(feature = "resolc")]
use self::settings::warning::Warning;
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
    #[serde(default, skip_serializing)]
    pub suppressed_warnings: Vec<Warning>,
}

impl Input {
    /// A shortcut constructor.
    ///
    /// If the `path` is `None`, the input is read from the stdin.
    pub fn try_from(path: Option<&Path>) -> anyhow::Result<Self> {
        let input_json = match path {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|error| anyhow::anyhow!("Standard JSON file {path:?} reading: {error}")),
            None => std::io::read_to_string(std::io::stdin())
                .map_err(|error| anyhow::anyhow!("Standard JSON reading from stdin: {error}")),
        }?;
        revive_common::deserialize_from_str::<Self>(input_json.as_str())
            .map_err(|error| anyhow::anyhow!("Standard JSON parsing: {error}"))
    }

    /// A shortcut constructor from paths.
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_solidity_paths(
        evm_version: Option<revive_common::EVMVersion>,
        paths: &[PathBuf],
        libraries: &[String],
        remappings: Option<BTreeSet<String>>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: Option<SolcStandardJsonInputSettingsMetadata>,
        #[cfg(feature = "resolc")] suppressed_warnings: Vec<Warning>,
        polkavm: Option<SolcStandardJsonInputSettingsPolkaVM>,
        detect_missing_libraries: bool,
    ) -> anyhow::Result<Self> {
        let mut paths: BTreeSet<PathBuf> = paths.iter().cloned().collect();
        let libraries = SolcStandardJsonInputSettingsLibraries::try_from(libraries)?;
        for library_file in libraries.as_inner().keys() {
            paths.insert(PathBuf::from(library_file));
        }

        #[cfg(feature = "parallel")]
        let iter = paths.into_par_iter(); // Parallel iterator

        #[cfg(not(feature = "parallel"))]
        let iter = paths.into_iter(); // Sequential iterator

        let sources = iter
            .map(|path| {
                let source = Source::try_read(path.as_path())?;
                Ok((path.to_string_lossy().to_string(), source))
            })
            .collect::<anyhow::Result<BTreeMap<String, Source>>>()?;

        Self::try_from_solidity_sources(
            evm_version,
            sources,
            libraries,
            remappings,
            output_selection,
            optimizer,
            metadata,
            suppressed_warnings,
            polkavm,
            detect_missing_libraries,
        )
    }

    /// A shortcut constructor from source code.
    /// Only for the integration test purposes.
    #[cfg(feature = "resolc")]
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_solidity_sources(
        evm_version: Option<revive_common::EVMVersion>,
        sources: BTreeMap<String, Source>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        remappings: Option<BTreeSet<String>>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: Option<SolcStandardJsonInputSettingsMetadata>,
        suppressed_warnings: Vec<Warning>,
        polkavm: Option<SolcStandardJsonInputSettingsPolkaVM>,
        detect_missing_libraries: bool,
    ) -> anyhow::Result<Self> {
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
                suppressed_warnings.clone(),
                detect_missing_libraries,
            ),
            suppressed_warnings,
        })
    }

    /// Sets the necessary defaults.
    pub fn normalize(&mut self) {
        self.settings.normalize();
    }
}
