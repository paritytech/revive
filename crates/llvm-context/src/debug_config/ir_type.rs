//! The debug IR type.

use revive_common::{
    EXTENSION_LLVM_SOURCE, EXTENSION_NEWYORK, EXTENSION_NEWYORK_JSON, EXTENSION_NEWYORK_MANIFEST,
    EXTENSION_OBJECT, EXTENSION_POLKAVM_ASSEMBLY, EXTENSION_YUL, EXTENSION_YUL_JSON,
};

/// The debug IR type.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IRType {
    /// Whether to dump the Yul code.
    Yul,
    /// Whether to dump the LLVM IR code.
    LLVM,
    /// Whether to dump the assembly code.
    Assembly,
    /// Whether to dump the ELF shared object
    Object,
    /// Whether to dump the newyork IR.
    NewYork,
    /// Whether to dump a machine-readable newyork IR stage artifact.
    NewYorkStage,
    /// Whether to dump the newyork IR stage manifest.
    NewYorkManifest,
    /// Whether to dump the machine-readable Yul AST.
    YulAst,
}

impl IRType {
    /// Returns the file extension for the specified IR.
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Yul => EXTENSION_YUL,
            Self::LLVM => EXTENSION_LLVM_SOURCE,
            Self::Assembly => EXTENSION_POLKAVM_ASSEMBLY,
            Self::Object => EXTENSION_OBJECT,
            Self::NewYork => EXTENSION_NEWYORK,
            Self::NewYorkStage => EXTENSION_NEWYORK_JSON,
            Self::NewYorkManifest => EXTENSION_NEWYORK_MANIFEST,
            Self::YulAst => EXTENSION_YUL_JSON,
        }
    }
}
