//! The `solc --standard-json` input settings.

pub mod libraries;
pub mod metadata;
pub mod metadata_hash;
pub mod optimizer;
pub mod polkavm;
pub mod selection;
#[cfg(feature = "resolc")]
pub mod warning;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use self::libraries::Libraries;
use self::metadata::Metadata;
use self::optimizer::Optimizer;
use self::polkavm::PolkaVM;
use self::selection::Selection;
#[cfg(feature = "resolc")]
use self::warning::Warning;

/// The `solc --standard-json` input settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The target EVM version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_version: Option<revive_common::EVMVersion>,
    /// The linker library addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libraries: Option<Libraries>,
    /// The sorted list of remappings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remappings: Option<BTreeSet<String>>,
    /// The output selection filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_selection: Option<Selection>,
    /// Whether to compile via IR. Only for testing with solc >=0.8.13.
    #[serde(
        rename = "viaIR",
        skip_serializing_if = "Option::is_none",
        skip_deserializing
    )]
    pub via_ir: Option<bool>,
    /// The optimizer settings.
    pub optimizer: Optimizer,
    /// The metadata settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// The resolc custom PolkaVM settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polkavm: Option<PolkaVM>,

    /// The suppressed warnings.
    #[cfg(feature = "resolc")]
    #[serde(default, skip_serializing)]
    pub suppressed_warnings: Vec<self::warning::Warning>,

    /// Whether to enable the missing libraries detection mode.
    /// Deprecated in favor of post-compile-time linking.
    #[serde(default, rename = "detectMissingLibraries", skip_serializing)]
    pub detect_missing_libraries: bool,
}

#[cfg(feature = "resolc")]
impl Settings {
    /// A shortcut constructor.
    pub fn new(
        evm_version: Option<revive_common::EVMVersion>,
        libraries: Libraries,
        remappings: Option<BTreeSet<String>>,
        output_selection: Selection,
        optimizer: Optimizer,
        metadata: Option<Metadata>,
        polkavm: Option<PolkaVM>,
        suppressed_warnings: Vec<Warning>,
        detect_missing_libraries: bool,
    ) -> Self {
        Self {
            evm_version,
            libraries: Some(libraries),
            remappings,
            output_selection: Some(output_selection),
            optimizer,
            metadata,
            via_ir: Some(true),
            polkavm,
            suppressed_warnings,
            detect_missing_libraries,
        }
    }

    /// Sets the necessary defaults.
    pub fn normalize(&mut self) {
        self.polkavm = None;
        self.optimizer.normalize();
    }
}
