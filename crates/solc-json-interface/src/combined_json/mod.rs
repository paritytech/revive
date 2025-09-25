//! The `solc --combined-json` output.

pub mod contract;
pub mod selector;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use self::contract::Contract;

/// The `solc --combined-json` output.
#[derive(Debug, Serialize, Deserialize)]
pub struct CombinedJson {
    /// The contract entries.
    pub contracts: BTreeMap<String, Contract>,
    /// The list of source files.
    #[serde(default, rename = "sourceList", skip_serializing_if = "Vec::is_empty")]
    pub source_list: Vec<String>,
    /// The source code extra data, including the AST.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub sources: serde_json::Value,
    /// The `solc` compiler version.
    pub version: String,
    /// The `resolc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(feature = "resolc")]
    pub resolc_version: Option<String>,
}

#[cfg(feature = "resolc")]
impl CombinedJson {
    /// A shortcut constructor.
    pub fn new(solc_version: semver::Version, resolc_version: Option<String>) -> Self {
        Self {
            contracts: BTreeMap::new(),
            source_list: Vec::new(),
            sources: serde_json::Value::Null,
            version: solc_version.to_string(),
            resolc_version,
        }
    }

    /// Writes the JSON to the specified directory.
    pub fn write_to_directory(
        self,
        output_directory: &std::path::Path,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let mut file_path = output_directory.to_owned();
        file_path.push(format!("combined.{}", revive_common::EXTENSION_JSON));

        if file_path.exists() && !overwrite {
            anyhow::bail!(
                "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
            );
        }

        std::fs::write(
            file_path.as_path(),
            serde_json::to_vec(&self).expect("Always valid").as_slice(),
        )
        .map_err(|error| anyhow::anyhow!("File {file_path:?} writing: {error}"))?;

        Ok(())
    }
}
