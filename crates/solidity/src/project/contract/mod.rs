//! The contract data.

pub mod ir;
pub mod metadata;

use std::collections::HashSet;

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
    pub path: String,
    /// The IR source code data.
    pub ir: IR,
    /// The metadata JSON.
    pub metadata_json: serde_json::Value,
}

impl Contract {
    /// A shortcut constructor.
    pub fn new(
        path: String,
        source_hash: [u8; revive_common::BYTE_LENGTH_WORD],
        source_version: SolcVersion,
        ir: IR,
        metadata_json: Option<serde_json::Value>,
    ) -> Self {
        let metadata_json = metadata_json.unwrap_or_else(|| {
            serde_json::json!({
                "source_hash": hex::encode(source_hash.as_slice()),
                "source_version": serde_json::to_value(&source_version).expect("Always valid"),
            })
        });

        Self {
            path,
            ir,
            metadata_json,
        }
    }

    /// Returns the contract identifier, which is:
    /// - the Yul object identifier for Yul
    /// - the module name for LLVM IR
    pub fn identifier(&self) -> &str {
        match self.ir {
            IR::Yul(ref yul) => yul.object.identifier.as_str(),
            IR::LLVMIR(ref llvm_ir) => llvm_ir.path.as_str(),
        }
    }

    /// Extract factory dependencies.
    pub fn drain_factory_dependencies(&mut self) -> HashSet<String> {
        match self.ir {
            IR::Yul(ref mut yul) => yul.object.factory_dependencies.drain().collect(),
            IR::LLVMIR(_) => HashSet::new(),
        }
    }

    /// Compiles the specified contract, setting its build artifacts.
    pub fn compile(
        mut self,
        project: Project,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        include_metadata_hash: bool,
        debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: &[String],
        memory_config: revive_llvm_context::MemoryConfig,
    ) -> anyhow::Result<ContractBuild> {
        let llvm = inkwell::context::Context::create();
        let optimizer = revive_llvm_context::Optimizer::new(optimizer_settings);

        let version = project.version.clone();
        let identifier = self.identifier().to_owned();

        let metadata = Metadata::new(
            self.metadata_json.take(),
            version.long.clone(),
            version.l2_revision.clone(),
            optimizer.settings().to_owned(),
            llvm_arguments.to_vec(),
        );
        let metadata_json = serde_json::to_value(&metadata).expect("Always valid");
        let metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]> = if include_metadata_hash
        {
            let metadata_string = serde_json::to_string(&metadata).expect("Always valid");
            Some(sha3::Keccak256::digest(metadata_string.as_bytes()).into())
        } else {
            None
        };

        let module = match self.ir {
            IR::LLVMIR(ref llvm_ir) => {
                // Create the output module
                let memory_buffer =
                    inkwell::memory_buffer::MemoryBuffer::create_from_memory_range_copy(
                        llvm_ir.source.as_bytes(),
                        self.path.as_str(),
                    );
                llvm.create_module_from_ir(memory_buffer)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?
            }
            _ => llvm.create_module(self.path.as_str()),
        };

        let mut context = revive_llvm_context::PolkaVMContext::new(
            &llvm,
            module,
            optimizer,
            Some(project),
            include_metadata_hash,
            debug_config,
            llvm_arguments,
            memory_config,
        );
        context.set_solidity_data(revive_llvm_context::PolkaVMContextSolidityData::default());
        match self.ir {
            IR::Yul(_) => {
                context.set_yul_data(Default::default());
            }
            IR::LLVMIR(_) => {}
        }

        let factory_dependencies = self.drain_factory_dependencies();

        self.ir.declare(&mut context).map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` LLVM IR generator declaration pass error: {}",
                self.path,
                error
            )
        })?;
        self.ir.into_llvm(&mut context).map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` LLVM IR generator definition pass error: {}",
                self.path,
                error
            )
        })?;

        if let Some(debug_info) = context.debug_info() {
            debug_info.finalize_module()
        }

        let build = context.build(self.path.as_str(), metadata_hash)?;

        Ok(ContractBuild::new(
            self.path,
            identifier,
            build,
            metadata_json,
            factory_dependencies,
        ))
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.ir.get_missing_libraries()
    }
}

impl<D> PolkaVMWriteLLVM<D> for Contract
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<D>,
    ) -> anyhow::Result<()> {
        self.ir.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        self.ir.into_llvm(context)
    }
}
