//! The contract source code.

pub mod yul;

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use self::yul::Yul;

/// The contract source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum IR {
    /// The Yul source code.
    Yul(Yul),
}

impl IR {
    /// Drains the list of factory dependencies.
    pub fn drain_factory_dependencies(&mut self) -> BTreeSet<String> {
        match self {
            IR::Yul(ref mut yul) => yul.object.factory_dependencies.drain().collect(),
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
        }
    }
}

impl From<Yul> for IR {
    fn from(inner: Yul) -> Self {
        Self::Yul(inner)
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for IR {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.declare(context),
        }
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.into_llvm(context),
        }
    }
}
