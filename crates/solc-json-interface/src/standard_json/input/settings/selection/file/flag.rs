//! The `solc --standard-json` expected output selection flag.

use serde::Deserialize;
use serde::Serialize;

/// The `solc --standard-json` expected output selection flag.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flag {
    /// The ABI JSON.
    #[serde(rename = "abi")]
    ABI,
    /// The metadata.
    #[serde(rename = "metadata")]
    Metadata,
    /// The developer documentation.
    #[serde(rename = "devdoc")]
    Devdoc,
    /// The user documentation.
    #[serde(rename = "userdoc")]
    Userdoc,
    /// The function signature hashes JSON.
    #[serde(rename = "evm.methodIdentifiers")]
    MethodIdentifiers,
    /// The storage layout.
    #[serde(rename = "storageLayout")]
    StorageLayout,
    /// The AST JSON.
    #[serde(rename = "ast")]
    AST,
    /// The Yul IR.
    #[serde(rename = "irOptimized")]
    Yul,
    /// The EVM bytecode.
    #[serde(rename = "evm")]
    EVM,
    /// The EVM legacy assembly JSON.
    #[serde(rename = "evm.legacyAssembly")]
    EVMLA,
    #[serde(rename = "evm.bytecode")]
    EVMBC,
    #[serde(rename = "evm.deployedBytecode")]
    EVMDBC,
    /// The assembly code
    #[serde(rename = "evm.assembly")]
    Assembly,
    /// The Ir
    #[serde(rename = "ir")]
    Ir,
}

impl Flag {
    /// Returns all flag variants.
    pub fn all() -> &'static [Self] {
        &[
            Self::ABI,
            Self::Metadata,
            Self::Devdoc,
            Self::Userdoc,
            Self::MethodIdentifiers,
            Self::StorageLayout,
            Self::AST,
            Self::Yul,
            Self::EVM,
            Self::EVMLA,
            Self::EVMBC,
            Self::EVMDBC,
            Self::Assembly,
            Self::Ir,
        ]
    }

    /// Returns the EVM child flag variants.
    pub fn evm_children() -> &'static [Self] {
        &[
            Self::MethodIdentifiers,
            Self::EVMLA,
            Self::EVMBC,
            Self::EVMDBC,
            Self::Assembly,
        ]
    }

    /// Whether this selection flag is required for the revive codegen.
    ///
    /// Specifically, EVM bytecode and related flags should never be requested.
    /// It will be replaced by PVM code anyways.
    pub fn is_required_for_codegen(&self) -> bool {
        !matches!(
            self,
            Flag::EVMBC | Flag::EVMDBC | Flag::EVMLA | Flag::EVM | Flag::Assembly
        )
    }
}
