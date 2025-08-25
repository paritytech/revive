//! The contract Yul source code.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use revive_yul::parser::statement::object::Object;

/// The contract Yul source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Yul {
    /// The Yul source code.
    pub source_code: String,
    /// The Yul AST object.
    pub object: Object,
}

impl Yul {
    /// A shortcut constructor.
    pub fn new(source_code: String, object: Object) -> Self {
        Self {
            source_code,
            object,
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        self.object.get_missing_libraries()
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for Yul {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.object.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.object.into_llvm(context)
    }
}
