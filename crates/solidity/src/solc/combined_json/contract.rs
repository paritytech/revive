//!
//! The `solc --combined-json` contract.
//!

use std::collections::BTreeMap;
use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

///
/// The contract.
///
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Contract {
    /// The `solc` hashes output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<BTreeMap<String, String>>,
    /// The `solc` ABI output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abi: Option<serde_json::Value>,
    /// The `solc` metadata output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// The `solc` developer documentation output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devdoc: Option<serde_json::Value>,
    /// The `solc` user documentation output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userdoc: Option<serde_json::Value>,
    /// The `solc` storage layout output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_layout: Option<serde_json::Value>,
    /// The `solc` AST output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast: Option<serde_json::Value>,
    /// The `solc` assembly output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asm: Option<serde_json::Value>,
    /// The `solc` hexadecimal binary output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    /// The `solc` hexadecimal binary runtime part output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_runtime: Option<String>,
    /// The factory dependencies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory_deps: Option<BTreeMap<String, String>>,
    /// The missing libraries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_libraries: Option<HashSet<String>>,
}

impl Contract {
    ///
    /// Returns the signature hash of the specified contract entry.
    ///
    /// # Panics
    /// If the hashes have not been requested in the `solc` call.
    ///
    pub fn entry(&self, entry: &str) -> u32 {
        self.hashes
            .as_ref()
            .expect("Always exists")
            .iter()
            .find_map(|(contract_entry, hash)| {
                if contract_entry.starts_with(entry) {
                    Some(
                        u32::from_str_radix(hash.as_str(), era_compiler_common::BASE_HEXADECIMAL)
                            .expect("Test hash is always valid"),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Entry `{entry}` not found"))
    }
}
