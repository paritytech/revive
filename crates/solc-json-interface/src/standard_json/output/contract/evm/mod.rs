//! The `solc --standard-json` output contract EVM data.

pub mod bytecode;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use self::bytecode::Bytecode;
use self::bytecode::DeployedBytecode;

/// The `solc --standard-json` output contract EVM data.
/// It is replaced by PolkaVM data after compiling.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EVM {
    /// The contract PolkaVM assembly code.
    #[serde(rename = "assembly", skip_serializing_if = "Option::is_none")]
    pub assembly_text: Option<String>,
    /// The contract bytecode.
    /// Is reset by that of PolkaVM before yielding the compiled project artifacts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode: Option<Bytecode>,
    /// The deployed bytecode of the contract.
    /// It is overwritten with the PolkaVM blob before yielding the compiled project artifacts.
    /// Hence it will be the same as the runtime code but we keep both for compatibility reasons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployed_bytecode: Option<DeployedBytecode>,
    /// The contract function signatures.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub method_identifiers: BTreeMap<String, String>,
}

impl EVM {
    /// Sets the PolkaVM assembly and bytecode.
    pub fn modify(&mut self, assembly_text: String, bytecode: String) {
        self.assembly_text = Some(assembly_text);
        self.bytecode = Some(Bytecode::new(bytecode.clone()));
        self.deployed_bytecode = Some(DeployedBytecode::new(bytecode));
    }
}
