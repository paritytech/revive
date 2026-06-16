//! The contract data.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use revive_common::ContractIdentifier;
use revive_common::Keccak256;
use revive_common::MetadataHash;
use revive_common::ObjectFormat;
use revive_llvm_context::DebugConfig;
use revive_llvm_context::Optimizer;
use revive_llvm_context::OptimizerSettings;
use revive_llvm_context::PolkaVMContext;
use revive_llvm_context::PolkaVMContextSolidityData;
use revive_llvm_context::PolkaVMContextYulData;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use serde::Deserialize;
use serde::Serialize;

use revive_llvm_context::PolkaVMWriteLLVM;

use crate::build::contract::Contract as ContractBuild;
use crate::solc::version::Version as SolcVersion;

use self::ir::IR;
use self::metadata::Metadata;

pub mod ir;
pub mod metadata;

/// The contract data.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Contract {
    /// The absolute file path.
    pub identifier: ContractIdentifier,
    /// The IR source code data.
    pub ir: IR,
    /// The metadata JSON.
    pub metadata_json: serde_json::Value,
}

impl Contract {
    /// A shortcut constructor.
    pub fn new(identifier: ContractIdentifier, ir: IR, metadata_json: serde_json::Value) -> Self {
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
            IR::NewYork(ref newyork) => newyork.yul_object.identifier.as_str(),
        }
    }

    /// Compiles the specified contract, setting its build artifacts.
    pub fn compile(
        self,
        solc_version: Option<SolcVersion>,
        optimizer_settings: OptimizerSettings,
        metadata_hash: MetadataHash,
        mut debug_config: DebugConfig,
        llvm_arguments: &[String],
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
        missing_libraries: BTreeSet<String>,
        factory_dependencies: BTreeSet<String>,
        identifier_paths: BTreeMap<String, String>,
    ) -> anyhow::Result<ContractBuild> {
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
        let metadata_bytes = match metadata_hash {
            MetadataHash::Keccak256 => Keccak256::from_slice(&metadata_json_bytes).into(),
            MetadataHash::IPFS => todo!("IPFS hash isn't supported yet"),
            MetadataHash::None => None,
        };
        debug_config.set_contract_path(&self.identifier.full_path);

        let full_path = self.identifier.full_path.as_str();
        let build = match self.ir {
            IR::Yul(yul) => compile_ir(
                yul,
                &llvm,
                optimizer,
                false,
                debug_config,
                memory_config,
                identifier_paths,
                full_path,
                metadata_bytes,
            )?,
            IR::NewYork(newyork) => compile_ir(
                newyork,
                &llvm,
                optimizer,
                true,
                debug_config,
                memory_config,
                identifier_paths,
                full_path,
                metadata_bytes,
            )?,
        };

        Ok(ContractBuild::new(
            self.identifier,
            build,
            metadata_json,
            missing_libraries,
            factory_dependencies,
            ObjectFormat::ELF,
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

/// Lowers an IR object to an LLVM module and links it into a PolkaVM build.
///
/// Shared by the `Yul` and `NewYork` arms of [`Contract::compile`], which differ
/// only in whether the newyork optimizer pipeline is enabled and in the error
/// label attached to a codegen failure.
fn compile_ir<T: PolkaVMWriteLLVM>(
    mut ir: T,
    llvm: &inkwell::context::Context,
    mut optimizer: Optimizer,
    use_newyork: bool,
    debug_config: DebugConfig,
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    identifier_paths: BTreeMap<String, String>,
    full_path: &str,
    metadata_bytes: Option<Keccak256>,
) -> anyhow::Result<revive_llvm_context::PolkaVMBuild> {
    if use_newyork {
        optimizer.enable_newyork_pipeline();
    }

    let module = llvm.create_module(full_path);
    let mut context = PolkaVMContext::new(llvm, module, optimizer, debug_config, memory_config);
    context.set_solidity_data(PolkaVMContextSolidityData::default());
    context.set_yul_data(PolkaVMContextYulData::new(identifier_paths));

    ir.declare(&mut context)?;
    let generator_error_label = if use_newyork {
        "NewYork IR generator"
    } else {
        "LLVM IR generator"
    };
    ir.into_llvm(&mut context)
        .map_err(|error| anyhow::anyhow!("{generator_error_label}: {error}"))?;

    context.build(full_path, metadata_bytes)
}
