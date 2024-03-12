//!
//! The contract data.
//!

pub mod ir;
pub mod metadata;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;
use sha3::Digest;

use era_compiler_llvm_context::EraVMWriteLLVM;

use crate::build::contract::Contract as ContractBuild;
use crate::project::Project;
use crate::solc::version::Version as SolcVersion;

use self::ir::IR;
use self::metadata::Metadata;

///
/// The contract data.
///
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
    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        path: String,
        source_hash: [u8; era_compiler_common::BYTE_LENGTH_FIELD],
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

    ///
    /// Returns the contract identifier, which is:
    /// - the Yul object identifier for Yul
    /// - the full contract path for EVM legacy assembly
    /// - the module name for LLVM IR
    ///
    pub fn identifier(&self) -> &str {
        match self.ir {
            IR::Yul(ref yul) => yul.object.identifier.as_str(),
            IR::EVMLA(ref evm) => evm.assembly.full_path(),
            IR::LLVMIR(ref llvm_ir) => llvm_ir.path.as_str(),
            IR::ZKASM(ref zkasm) => zkasm.path.as_str(),
        }
    }

    ///
    /// Extract factory dependencies.
    ///
    pub fn drain_factory_dependencies(&mut self) -> HashSet<String> {
        match self.ir {
            IR::Yul(ref mut yul) => yul.object.factory_dependencies.drain().collect(),
            IR::EVMLA(ref mut evm) => evm.assembly.factory_dependencies.drain().collect(),
            IR::LLVMIR(_) => HashSet::new(),
            IR::ZKASM(_) => HashSet::new(),
        }
    }

    ///
    /// Compiles the specified contract, setting its build artifacts.
    ///
    pub fn compile(
        mut self,
        project: Project,
        optimizer_settings: era_compiler_llvm_context::OptimizerSettings,
        is_system_mode: bool,
        include_metadata_hash: bool,
        debug_config: Option<era_compiler_llvm_context::DebugConfig>,
    ) -> anyhow::Result<ContractBuild> {
        let llvm = inkwell::context::Context::create();
        let optimizer = era_compiler_llvm_context::Optimizer::new(optimizer_settings);

        let version = project.version.clone();
        let identifier = self.identifier().to_owned();

        let metadata = Metadata::new(
            self.metadata_json.take(),
            version.default.clone(),
            version.l2_revision.clone(),
            semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Always valid"),
            optimizer.settings().to_owned(),
        );
        let metadata_json = serde_json::to_value(&metadata).expect("Always valid");
        let metadata_hash: Option<[u8; era_compiler_common::BYTE_LENGTH_FIELD]> =
            if include_metadata_hash {
                let metadata_string = serde_json::to_string(&metadata).expect("Always valid");
                Some(sha3::Keccak256::digest(metadata_string.as_bytes()).into())
            } else {
                None
            };

        let module = match self.ir {
            IR::LLVMIR(ref llvm_ir) => {
                let memory_buffer =
                    inkwell::memory_buffer::MemoryBuffer::create_from_memory_range_copy(
                        llvm_ir.source.as_bytes(),
                        self.path.as_str(),
                    );
                llvm.create_module_from_ir(memory_buffer)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?
            }
            IR::ZKASM(ref zkasm) => {
                let build = era_compiler_llvm_context::eravm_build_assembly_text(
                    self.path.as_str(),
                    zkasm.source.as_str(),
                    metadata_hash,
                    debug_config.as_ref(),
                )?;
                return Ok(ContractBuild::new(
                    self.path,
                    identifier,
                    build,
                    metadata_json,
                    HashSet::new(),
                ));
            }
            _ => llvm.create_module(self.path.as_str()),
        };
        let mut context = era_compiler_llvm_context::EraVMContext::new(
            &llvm,
            module,
            optimizer,
            Some(project),
            include_metadata_hash,
            debug_config,
        );
        context.set_solidity_data(era_compiler_llvm_context::EraVMContextSolidityData::default());
        match self.ir {
            IR::Yul(_) => {
                let yul_data = era_compiler_llvm_context::EraVMContextYulData::new(is_system_mode);
                context.set_yul_data(yul_data);
            }
            IR::EVMLA(_) => {
                let evmla_data =
                    era_compiler_llvm_context::EraVMContextEVMLAData::new(version.default);
                context.set_evmla_data(evmla_data);
            }
            IR::LLVMIR(_) => {}
            IR::ZKASM(_) => {}
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

        let build = context.build(self.path.as_str(), metadata_hash)?;

        Ok(ContractBuild::new(
            self.path,
            identifier,
            build,
            metadata_json,
            factory_dependencies,
        ))
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.ir.get_missing_libraries()
    }
}

impl<D> EraVMWriteLLVM<D> for Contract
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        self.ir.declare(context)
    }

    fn into_llvm(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        self.ir.into_llvm(context)
    }
}
