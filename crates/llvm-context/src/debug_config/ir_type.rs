//! The debug IR type.

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
    SO,
    /// Whether to jump JSON
    #[cfg(debug_assertions)]
    JSON,
}

impl IRType {
    /// Returns the file extension for the specified IR.
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Yul => revive_common::EXTENSION_YUL,
            Self::LLVM => revive_common::EXTENSION_LLVM_SOURCE,
            Self::Assembly => revive_common::EXTENSION_POLKAVM_ASSEMBLY,
            #[cfg(debug_assertions)]
            Self::JSON => revive_common::EXTENSION_JSON,
            Self::SO => revive_common::EXTENSION_SHARED_OBJECT,
        }
    }
}
