//! The contract source code.

pub mod llvm_ir;
pub mod yul;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use revive_yul::parser::statement::object::Object;

use self::llvm_ir::LLVMIR;
use self::yul::Yul;

/// The contract source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum IR {
    /// The Yul source code.
    Yul(Yul),
    /// The LLVM IR source code.
    LLVMIR(LLVMIR),
}

impl IR {
    /// A shortcut constructor.
    pub fn new_yul(source_code: String, object: Object) -> Self {
        Self::Yul(Yul::new(source_code, object))
    }

    /// A shortcut constructor.
    pub fn new_llvm_ir(path: String, source: String) -> Self {
        Self::LLVMIR(LLVMIR::new(path, source))
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
            Self::LLVMIR(_inner) => HashSet::new(),
        }
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for IR
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<D>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.declare(context),
            Self::LLVMIR(_inner) => Ok(()),
        }
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.into_llvm(context),
            Self::LLVMIR(_inner) => Ok(()),
        }
    }
}
