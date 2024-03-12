//!
//! The contract source code.
//!

pub mod evmla;
pub mod llvm_ir;
pub mod yul;
pub mod zkasm;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::evmla::assembly::Assembly;
use crate::solc::standard_json::output::contract::evm::extra_metadata::ExtraMetadata;
use crate::yul::parser::statement::object::Object;

use self::evmla::EVMLA;
use self::llvm_ir::LLVMIR;
use self::yul::Yul;
use self::zkasm::ZKASM;

///
/// The contract source code.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[allow(clippy::enum_variant_names)]
pub enum IR {
    /// The Yul source code.
    Yul(Yul),
    /// The EVM legacy assembly source code.
    EVMLA(EVMLA),
    /// The LLVM IR source code.
    LLVMIR(LLVMIR),
    /// The EraVM assembly source code.
    ZKASM(ZKASM),
}

impl IR {
    ///
    /// A shortcut constructor.
    ///
    pub fn new_yul(source_code: String, object: Object) -> Self {
        Self::Yul(Yul::new(source_code, object))
    }

    ///
    /// A shortcut constructor.
    ///
    pub fn new_evmla(assembly: Assembly, extra_metadata: ExtraMetadata) -> Self {
        Self::EVMLA(EVMLA::new(assembly, extra_metadata))
    }

    ///
    /// A shortcut constructor.
    ///
    pub fn new_llvm_ir(path: String, source: String) -> Self {
        Self::LLVMIR(LLVMIR::new(path, source))
    }

    ///
    /// A shortcut constructor.
    ///
    pub fn new_zkasm(path: String, source: String) -> Self {
        Self::ZKASM(ZKASM::new(path, source))
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
            Self::EVMLA(inner) => inner.get_missing_libraries(),
            Self::LLVMIR(_inner) => HashSet::new(),
            Self::ZKASM(_inner) => HashSet::new(),
        }
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for IR
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.declare(context),
            Self::EVMLA(inner) => inner.declare(context),
            Self::LLVMIR(_inner) => Ok(()),
            Self::ZKASM(_inner) => Ok(()),
        }
    }

    fn into_llvm(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Yul(inner) => inner.into_llvm(context),
            Self::EVMLA(inner) => inner.into_llvm(context),
            Self::LLVMIR(_inner) => Ok(()),
            Self::ZKASM(_inner) => Ok(()),
        }
    }
}
