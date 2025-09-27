//! The `solc --standard-json` input.

use std::collections::BTreeMap;
#[cfg(feature = "resolc")]
use std::collections::BTreeSet;
#[cfg(feature = "resolc")]
use std::path::Path;
#[cfg(feature = "resolc")]
use std::path::PathBuf;

#[cfg(all(feature = "parallel", feature = "resolc"))]
use rayon::iter::{IntoParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::metadata::Metadata as SolcStandardJsonInputSettingsMetadata;
#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSettingsLibraries;
#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSettingsPolkaVM;

use self::language::Language;
#[cfg(feature = "resolc")]
use self::settings::warning::Warning;
use self::settings::Settings;
use self::source::Source;

pub mod language;
pub mod settings;
pub mod source;

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

#[cfg(feature = "resolc")]
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
    pub fn try_from_solidity_paths(
        evm_version: Option<revive_common::EVMVersion>,
        paths: &[PathBuf],
        libraries: &[String],
        remappings: BTreeSet<String>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: SolcStandardJsonInputSettingsMetadata,
        suppressed_warnings: Vec<Warning>,
        polkavm: SolcStandardJsonInputSettingsPolkaVM,
        llvm_arguments: Vec<String>,
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
            llvm_arguments,
            detect_missing_libraries,
        )
    }

    /// A shortcut constructor from source code.
    /// Only for the integration test purposes.
    pub fn try_from_solidity_sources(
        evm_version: Option<revive_common::EVMVersion>,
        sources: BTreeMap<String, Source>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        remappings: BTreeSet<String>,
        output_selection: SolcStandardJsonInputSettingsSelection,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        metadata: SolcStandardJsonInputSettingsMetadata,
        suppressed_warnings: Vec<Warning>,
        polkavm: SolcStandardJsonInputSettingsPolkaVM,
        llvm_arguments: Vec<String>,
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
                llvm_arguments,
                detect_missing_libraries,
            ),
            suppressed_warnings,
        })
    }

    /// A shortcut constructor from paths to Yul source files.
    pub fn from_yul_paths(
        paths: &[PathBuf],
        libraries: SolcStandardJsonInputSettingsLibraries,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        llvm_options: Vec<String>,
    ) -> Self {
        let sources = paths
            .iter()
            .map(|path| {
                (
                    path.to_string_lossy().to_string(),
                    Source::from(path.as_path()),
                )
            })
            .collect();
        Self::from_yul_sources(sources, libraries, optimizer, llvm_options)
    }

    /// A shortcut constructor from Yul source code.
    pub fn from_yul_sources(
        sources: BTreeMap<String, Source>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        optimizer: SolcStandardJsonInputSettingsOptimizer,
        llvm_arguments: Vec<String>,
    ) -> Self {
        let output_selection = SolcStandardJsonInputSettingsSelection::new_yul_validation();

        Self {
            language: Language::Yul,
            sources,
            settings: Settings::new(
                None,
                libraries,
                Default::default(),
                output_selection,
                optimizer,
                Default::default(),
                Default::default(),
                vec![],
                llvm_arguments,
                false,
            ),
            suppressed_warnings: vec![],
        }
    }

    /// Extends the output selection with another one.
    pub fn extend_selection(&mut self, selection: SolcStandardJsonInputSettingsSelection) {
        self.settings.extend_selection(selection);
    }

    /// Tries to resolve all sources.
    pub fn resolve_sources(&mut self) {
        #[cfg(feature = "parallel")]
        let iter = self.sources.par_iter_mut();
        #[cfg(not(feature = "parallel"))]
        let iter = self.sources.iter_mut();

        iter.for_each(|(_path, source)| {
            let _ = source.try_resolve();
        });
    }
}
