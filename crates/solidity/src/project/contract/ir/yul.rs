//!
//! The contract Yul source code.
//!

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::yul::parser::statement::object::Object;

///
/// The contract Yul source code.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Yul {
    /// The Yul source code.
    pub source_code: String,
    /// The Yul AST object.
    pub object: Object,
}

impl Yul {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(source_code: String, object: Object) -> Self {
        Self {
            source_code,
            object,
        }
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.object.get_missing_libraries()
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for Yul
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        self.object.declare(context)
    }

    fn into_llvm(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        self.object.into_llvm(context)
    }
}
