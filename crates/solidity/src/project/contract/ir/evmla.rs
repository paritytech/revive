//! The contract EVM legacy assembly source code.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::evmla::assembly::Assembly;
use crate::solc::standard_json::output::contract::evm::extra_metadata::ExtraMetadata;

/// The contract EVM legacy assembly source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
pub struct EVMLA {
    /// The EVM legacy assembly source code.
    pub assembly: Assembly,
}

impl EVMLA {
    /// A shortcut constructor.
    pub fn new(mut assembly: Assembly, extra_metadata: ExtraMetadata) -> Self {
        assembly.extra_metadata = Some(extra_metadata);
        Self { assembly }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.assembly.get_missing_libraries()
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for EVMLA
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<D>,
    ) -> anyhow::Result<()> {
        self.assembly.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        self.assembly.into_llvm(context)
    }
}
