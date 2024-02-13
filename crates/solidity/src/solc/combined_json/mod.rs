//!
//! The `solc --combined-json` output.
//!

pub mod contract;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use self::contract::Contract;

///
/// The `solc --combined-json` output.
///
#[derive(Debug, Serialize, Deserialize)]
pub struct CombinedJson {
    /// The contract entries.
    pub contracts: BTreeMap<String, Contract>,
    /// The list of source files.
    #[serde(rename = "sourceList")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_list: Option<Vec<String>>,
    /// The source code extra data, including the AST.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<serde_json::Value>,
    /// The `solc` compiler version.
    pub version: String,
    /// The `zksolc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zk_version: Option<String>,
}

impl CombinedJson {
    ///
    /// Returns the signature hash of the specified contract and entry.
    ///
    pub fn entry(&self, path: &str, entry: &str) -> u32 {
        self.contracts
            .iter()
            .find_map(|(name, contract)| {
                if name.starts_with(path) {
                    Some(contract)
                } else {
                    None
                }
            })
            .expect("Always exists")
            .entry(entry)
    }

    ///
    /// Returns the full contract path which can be found in `combined-json` output.
    ///
    pub fn get_full_path(&self, name: &str) -> Option<String> {
        self.contracts.iter().find_map(|(path, _value)| {
            if let Some(last_slash_position) = path.rfind('/') {
                if let Some(colon_position) = path.rfind(':') {
                    if &path[last_slash_position + 1..colon_position] == name {
                        return Some(path.to_owned());
                    }
                }
            }

            None
        })
    }

    ///
    /// Removes EVM artifacts to prevent their accidental usage.
    ///
    pub fn remove_evm(&mut self) {
        for (_, contract) in self.contracts.iter_mut() {
            contract.bin = None;
            contract.bin_runtime = None;
        }
    }

    ///
    /// Writes the JSON to the specified directory.
    ///
    pub fn write_to_directory(
        self,
        output_directory: &Path,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let mut file_path = output_directory.to_owned();
        file_path.push(format!("combined.{}", era_compiler_common::EXTENSION_JSON));

        if file_path.exists() && !overwrite {
            eprintln!(
                "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
            );
            return Ok(());
        }

        File::create(&file_path)
            .map_err(|error| anyhow::anyhow!("File {:?} creating error: {}", file_path, error))?
            .write_all(serde_json::to_vec(&self).expect("Always valid").as_slice())
            .map_err(|error| anyhow::anyhow!("File {:?} writing error: {}", file_path, error))?;

        Ok(())
    }
}
