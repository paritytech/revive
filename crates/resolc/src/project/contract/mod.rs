//! The contract data.

pub mod ir;
pub mod metadata;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;

use revive_common::Keccak256;
use revive_llvm_context::Optimizer;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use serde::Deserialize;
use serde::Serialize;
use sha3::Digest;

use revive_llvm_context::PolkaVMWriteLLVM;

use crate::build::contract::Contract as ContractBuild;
use crate::project::Project;
use crate::solc::version::Version as SolcVersion;

use self::ir::IR;
use self::metadata::Metadata;

/// The contract data.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Contract {
    /// The absolute file path.
    pub identifier: revive_common::ContractIdentifier,
    /// The IR source code data.
    pub ir: IR,
    /// The metadata JSON.
    pub metadata_json: serde_json::Value,
}

impl Contract {
    /// A shortcut constructor.
    pub fn new(
        identifier: revive_common::ContractIdentifier,
        ir: IR,
        metadata_json: serde_json::Value,
    ) -> Self {
        Self {
            identifier,
            ir,
            metadata_json,
        }
    }

    /// Returns the contract identifier, which is:
    /// - the Yul object identifier for Yul
    /// - the module name for LLVM IR
    pub fn object_identifier(&self) -> &str {
        match self.ir {
            IR::Yul(ref yul) => yul.object.identifier.as_str(),
            IR::LLVMIR(ref llvm_ir) => llvm_ir.path.as_str(),
        }
    }

    /// Compiles the specified contract, setting its build artifacts.
    pub fn compile(
        mut self,
        project: Project,
        solc_version: Option<SolcVersion>,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        include_metadata_hash: bool,
        mut debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: &[String],
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
        missing_libraries: BTreeSet<String>,
        factory_dependencies: BTreeSet<String>,
        identifier_paths: BTreeMap<String, String>,
    ) -> anyhow::Result<ContractBuild> {
        use revive_llvm_context::PolkaVMWriteLLVM;

        let llvm = inkwell::context::Context::create();
        let optimizer = Optimizer::new(optimizer_settings);

        let metadata = Metadata::new(
            self.metadata_json,
            solc_version
                .as_ref()
                .map(|version| version.default.to_owned()),
            optimizer.settings().to_owned(),
            llvm_arguments.to_owned(),
        );
        let metadata_json = serde_json::to_value(&metadata).expect("Always valid");
        let metadata_json_bytes = serde_json::to_vec(&metadata_json).expect("Always valid");
        let metadata_bytes = Keccak256::from_slice(&metadata_json_bytes).into();

        let build = match self.ir {
            IR::Yul(mut yul) => {
                let module = llvm.create_module(self.identifier.full_path.as_str());
                let mut context: revive_llvm_context::PolkaVMContext =
                    revive_llvm_context::PolkaVMContext::new(
                        &llvm,
                        module,
                        optimizer,
                        debug_config,
                        llvm_arguments,
                        memory_config,
                    );
                context
                    .set_solidity_data(revive_llvm_context::PolkaVMContextSolidityData::default());
                let yul_data = revive_llvm_context::PolkaVMContextYulData::new(identifier_paths);
                context.set_yul_data(yul_data);

                yul.declare(&mut context)?;
                yul.into_llvm(&mut context)
                    .map_err(|error| anyhow::anyhow!("LLVM IR generator: {error}"))?;

                context.build(self.identifier.full_path.as_str(), metadata_bytes)?
            }
            IR::LLVMIR(mut llvm_ir) => {
                llvm_ir.source.push(char::from(0));
                let memory_buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                    &llvm_ir.source.as_bytes()[..llvm_ir.source.len() - 1],
                    self.identifier.full_path.as_str(),
                );

                let module = llvm
                    .create_module_from_ir(memory_buffer)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let context: revive_llvm_context::PolkaVMContext =
                    revive_llvm_context::PolkaVMContext::new(
                        &llvm,
                        module,
                        optimizer,
                        debug_config,
                        llvm_arguments,
                        memory_config,
                    );

                context.build(self.identifier.full_path.as_str(), metadata_bytes)?
            }
        };

        Ok(ContractBuild::new(
            self.identifier,
            build,
            metadata_json,
            missing_libraries,
            factory_dependencies,
            revive_common::ObjectFormat::ELF,
        ))
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self, deployed_libraries: &BTreeSet<String>) -> BTreeSet<String> {
        self.ir
            .get_missing_libraries()
            .into_iter()
            .filter(|library| !deployed_libraries.contains(library))
            .collect::<BTreeSet<String>>()
    }
}

impl PolkaVMWriteLLVM for Contract {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.ir.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.ir.into_llvm(context)
    }
}
