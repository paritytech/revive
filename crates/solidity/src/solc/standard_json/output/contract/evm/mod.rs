//!
//! The `solc --standard-json` output contract EVM data.
//!

pub mod bytecode;
pub mod extra_metadata;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::evmla::assembly::Assembly;

use self::bytecode::Bytecode;
use self::extra_metadata::ExtraMetadata;

///
/// The `solc --standard-json` output contract EVM data.
///
/// It is replaced by EraVM data after compiling.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EVM {
    /// The contract EVM legacy assembly code.
    #[serde(rename = "legacyAssembly")]
    pub assembly: Option<Assembly>,
    /// The contract EraVM assembly code.
    #[serde(rename = "assembly")]
    pub assembly_text: Option<String>,
    /// The contract bytecode.
    /// Is reset by that of EraVM before yielding the compiled project artifacts.
    pub bytecode: Option<Bytecode>,
    /// The contract function signatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_identifiers: Option<BTreeMap<String, String>>,
    /// The extra EVMLA metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_metadata: Option<ExtraMetadata>,
}

impl EVM {
    ///
    /// Sets the EraVM assembly and bytecode.
    ///
    pub fn modify(&mut self, assembly_text: String, bytecode: String) {
        self.assembly_text = Some(assembly_text);
        self.bytecode = Some(Bytecode::new(bytecode));
    }
}
