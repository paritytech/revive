//! The contract source code.

pub mod llvm_ir;
pub mod yul;

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

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
    /// Drains the list of factory dependencies.
    pub fn drain_factory_dependencies(&mut self) -> BTreeSet<String> {
        match self {
            IR::Yul(ref mut yul) => yul.object.factory_dependencies.drain().collect(),
            IR::LLVMIR(_) => BTreeSet::new(),
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
            Self::LLVMIR(_inner) => BTreeSet::new(),
        }
    }
}

impl From<Yul> for IR {
    fn from(inner: Yul) -> Self {
        Self::Yul(inner)
    }
}

impl From<LLVMIR> for IR {
    fn from(inner: LLVMIR) -> Self {
        Self::LLVMIR(inner)
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for IR {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.declare(context),
            Self::LLVMIR(_inner) => Ok(()),
        }
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.into_llvm(context),
            Self::LLVMIR(_inner) => Ok(()),
        }
    }
}
