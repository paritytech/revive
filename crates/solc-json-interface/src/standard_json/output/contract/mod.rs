//! The `solc --standard-json` output contract.

pub mod evm;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use self::evm::EVM;

/// The `solc --standard-json` output contract.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// The contract ABI.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub abi: serde_json::Value,
    /// The contract metadata.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    /// The contract developer documentation.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub devdoc: serde_json::Value,
    /// The contract user documentation.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub userdoc: serde_json::Value,
    /// The contract storage layout.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub storage_layout: serde_json::Value,
    /// The contract storage layout.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub transient_storage_layout: serde_json::Value,
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
    /// Unlinked factory dependencies.
    #[serde(default, skip_deserializing)]
    pub factory_dependencies_unlinked: BTreeSet<String>,
    /// The contract factory dependencies.
    #[serde(default, skip_deserializing)]
    pub factory_dependencies: BTreeMap<String, String>,
    /// Missing linkable libraries.
    #[serde(default, skip_deserializing)]
    pub missing_libraries: BTreeSet<String>,
}

impl Contract {
    /// Checks if all fields are unset or empty.
    pub fn is_empty(&self) -> bool {
        self.abi.is_null()
            && self.storage_layout.is_null()
            && self.transient_storage_layout.is_null()
            && self.metadata.is_null()
            && self.devdoc.is_null()
            && self.userdoc.is_null()
            && self.ir_optimized.is_empty()
            && self.evm.is_none()
            && self.hash.is_none()
            && self.factory_dependencies_unlinked.is_empty()
            && self.factory_dependencies.is_empty()
            && self.missing_libraries.is_empty()
    }
}
