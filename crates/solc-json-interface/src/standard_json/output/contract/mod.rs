//! The `solc --standard-json` output contract.

pub mod evm;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use self::evm::EVM;

/// The `solc --standard-json` output contract.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// The contract ABI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi: Option<serde_json::Value>,
    /// The contract metadata.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    /// The contract developer documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devdoc: Option<serde_json::Value>,
    /// The contract user documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userdoc: Option<serde_json::Value>,
    /// The contract storage layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_layout: Option<serde_json::Value>,
    /// Contract's bytecode and related objects
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm: Option<EVM>,
    /// The contract IR code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<String>,
    /// The contract optimized IR code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ir_optimized: String,
    /// The contract PolkaVM bytecode hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// The contract factory dependencies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies: Option<BTreeMap<String, String>>,
    /// Missing linkable libraries.
    #[serde(default, skip_deserializing)]
    pub missing_libraries: BTreeSet<String>,
}
