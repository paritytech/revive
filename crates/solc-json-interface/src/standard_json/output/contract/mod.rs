//! The `solc --standard-json` output contract.

pub mod evm;

#[cfg(feature = "resolc")]
use std::collections::BTreeMap;
#[cfg(feature = "resolc")]
use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::SolcStandardJsonInputSettingsSelectionFileFlag;

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
    #[cfg(feature = "resolc")]
    #[serde(default, skip_deserializing)]
    pub factory_dependencies_unlinked: BTreeSet<String>,
    /// The contract factory dependencies.
    #[cfg(feature = "resolc")]
    #[serde(default, skip_deserializing)]
    pub factory_dependencies: BTreeMap<String, String>,
    /// Missing linkable libraries.
    #[cfg(feature = "resolc")]
    #[serde(default, skip_deserializing)]
    pub missing_libraries: BTreeSet<String>,
    /// Binary object format.
    #[cfg(feature = "resolc")]
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub object_format: Option<revive_common::ObjectFormat>,
}

#[cfg(feature = "resolc")]
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

    /// Sets the field corresponding to the given `flag` to its default value,
    /// resulting in the field being skipped during serialization.
    pub fn reset_field_by_flag(&mut self, flag: SolcStandardJsonInputSettingsSelectionFileFlag) {
        match flag {
            SolcStandardJsonInputSettingsSelectionFileFlag::ABI => {
                self.abi = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Metadata => {
                self.metadata = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Devdoc => {
                self.devdoc = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Userdoc => {
                self.userdoc = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::StorageLayout => {
                self.storage_layout = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Yul => {
                self.ir_optimized = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Ir => {
                self.ir = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::EVM => {
                self.evm = Default::default();
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::EVMBC => {
                if let Some(evm) = self.evm.as_mut() {
                    evm.bytecode = Default::default();
                }
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::EVMDBC => {
                if let Some(evm) = self.evm.as_mut() {
                    evm.deployed_bytecode = Default::default();
                }
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::Assembly => {
                if let Some(evm) = self.evm.as_mut() {
                    evm.assembly_text = Default::default();
                }
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::MethodIdentifiers => {
                if let Some(evm) = self.evm.as_mut() {
                    evm.method_identifiers = Default::default();
                }
            }
            SolcStandardJsonInputSettingsSelectionFileFlag::AST
            | SolcStandardJsonInputSettingsSelectionFileFlag::EVMLA => {
                // Ignore AST (a per-file flag) and EVMLA
                // as they have no contract field mappings.
            }
        }
    }
}
