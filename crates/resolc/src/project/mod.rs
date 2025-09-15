//! The processed input data.

pub mod contract;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "parallel")]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use revive_common::Keccak256;
use revive_common::MetadataHash;
use revive_llvm_context::DebugConfig;
use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use revive_solc_json_interface::SolcStandardJsonInputSource;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use serde::Deserialize;
use serde::Serialize;
use sha3::Digest;

use revive_common::ContractIdentifier;
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_yul::lexer::Lexer;
use revive_yul::parser::statement::object::Object;

use crate::build::contract::Contract as ContractBuild;
use crate::build::Build;
use crate::missing_libraries::MissingLibraries;
use crate::process::input::Input as ProcessInput;
use crate::process::Process;
use crate::project::contract::ir::llvm_ir::LLVMIR;
use crate::project::contract::ir::yul::Yul;
use crate::project::contract::ir::IR;
use crate::solc::version::Version as SolcVersion;
use crate::solc::Compiler;
use crate::ProcessOutput;

use self::contract::Contract;

/// The processes input data.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    /// The source code version.
    pub version: Option<SolcVersion>,
    /// The project contracts,
    pub contracts: BTreeMap<String, Contract>,
    /// The mapping of auxiliary identifiers, e.g. Yul object names, to full contract paths.
    pub identifier_paths: BTreeMap<String, String>,
    /// The library addresses.
    pub libraries: SolcStandardJsonInputSettingsLibraries,
}

impl Project {
    /// A shortcut constructor.
    pub fn new(
        version: Option<SolcVersion>,
        contracts: BTreeMap<String, Contract>,
        libraries: SolcStandardJsonInputSettingsLibraries,
    ) -> Self {
        let mut identifier_paths = BTreeMap::new();
        for (path, contract) in contracts.iter() {
            identifier_paths.insert(contract.object_identifier().to_owned(), path.to_owned());
        }

        Self {
            version,
            contracts,
            identifier_paths,
            libraries,
        }
    }

    /// Compiles all contracts, returning their build artifacts.
    pub fn compile(
        self,
        messages: &mut Vec<SolcStandardJsonOutputError>,
        optimizer_settings: OptimizerSettings,
        metadata_hash: MetadataHash,
        debug_config: DebugConfig,
        llvm_arguments: &[String],
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    ) -> anyhow::Result<Build> {
        let deployed_libraries = self.libraries.as_paths();

        #[cfg(feature = "parallel")]
        let iter = self.contracts.into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = self.contracts.into_iter();

        let results = iter
            .map(|(path, mut contract)| {
                let factory_dependencies = contract
                    .ir
                    .drain_factory_dependencies()
                    .iter()
                    .map(|identifier| {
                        self.identifier_paths
                            .get(identifier)
                            .cloned()
                            .expect("Always exists")
                    })
                    .collect();
                let missing_libraries = contract.get_missing_libraries(&deployed_libraries);
                let input = ProcessInput::new(
                    contract,
                    metadata_hash.clone(),
                    optimizer_settings.clone(),
                    debug_config.clone(),
                    llvm_arguments.to_owned(),
                    memory_config.clone(),
                    missing_libraries,
                    factory_dependencies,
                    self.identifier_paths.clone(),
                );
                let result: Result<ProcessOutput, SolcStandardJsonOutputError> = {
                    #[cfg(target_os = "emscripten")]
                    {
                        crate::WorkerProcess::call(path.as_str(), input)
                    }
                    #[cfg(not(target_os = "emscripten"))]
                    {
                        crate::NativeProcess::call(path.as_str(), input)
                    }
                };
                let result = result.map(|output| output.build);
                (path, result)
            })
            .collect::<BTreeMap<String, Result<ContractBuild, SolcStandardJsonOutputError>>>();
        Ok(Build::new(results, messages))
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self, deployed_libraries: &BTreeSet<String>) -> MissingLibraries {
        let missing_libraries = self
            .contracts
            .iter()
            .map(|(path, contract)| {
                (
                    path.to_owned(),
                    contract.get_missing_libraries(deployed_libraries),
                )
            })
            .collect();
        MissingLibraries::new(missing_libraries)
    }

    /// Parses the Yul source code file and returns the source data.
    pub fn try_from_yul_paths(
        paths: &[PathBuf],
        solc_output: Option<&mut SolcStandardJsonOutput>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        debug_config: &DebugConfig,
    ) -> anyhow::Result<Self> {
        let sources = paths
            .iter()
            .map(|path| {
                let source = SolcStandardJsonInputSource::from(path.as_path());
                (path.to_string_lossy().to_string(), source)
            })
            .collect::<BTreeMap<String, SolcStandardJsonInputSource>>();
        Self::try_from_yul_sources(sources, libraries, solc_output, debug_config)
    }

    /// Parses the test Yul source code string and returns the source data.
    /// Only for integration testing purposes.
    pub fn try_from_yul_sources(
        sources: BTreeMap<String, SolcStandardJsonInputSource>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        mut solc_output: Option<&mut SolcStandardJsonOutput>,
        debug_config: &DebugConfig,
    ) -> anyhow::Result<Self> {
        #[cfg(feature = "parallel")]
        let iter = sources.into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = sources.into_iter();

        let results = iter
            .filter_map(|(path, mut source)| {
                let source_code = match source.try_resolve() {
                    Ok(()) => source.take_content().expect("Always exists"),
                    Err(error) => return Some((path, Err(error))),
                };
                let ir =
                    match Yul::try_from_source(path.as_str(), source_code.as_str(), debug_config) {
                        Ok(ir) => ir?,
                        Err(error) => return Some((path, Err(error))),
                    };

                let source_hash = Keccak256::from_slice(source_code.as_bytes());
                let source_metadata = serde_json::json!({
                    "source_hash": source_hash.to_string(),
                });

                let name =
                    ContractIdentifier::new(path.clone(), Some(ir.object.identifier.clone()));
                let full_path = name.full_path.clone();
                let contract = Contract::new(name, ir.into(), source_metadata);
                Some((full_path, Ok(contract)))
            })
            .collect::<BTreeMap<String, anyhow::Result<Contract>>>();

        let mut contracts = BTreeMap::new();
        for (path, result) in results.into_iter() {
            match result {
                Ok(contract) => {
                    contracts.insert(path, contract);
                }
                Err(error) => match solc_output {
                    Some(ref mut solc_output) => solc_output.push_error(Some(path), error),
                    None => anyhow::bail!(error),
                },
            }
        }
        Ok(Self::new(None, contracts, libraries))
        // SolcStandardJsonInputLanguage::Yul,
    }

    /// Parses the LLVM IR source code file and returns the source data.
    pub fn try_from_llvm_ir_paths(
        paths: &[PathBuf],
        libraries: SolcStandardJsonInputSettingsLibraries,
        solc_output: Option<&mut SolcStandardJsonOutput>,
    ) -> anyhow::Result<Self> {
        let sources = paths
            .iter()
            .map(|path| {
                let source = SolcStandardJsonInputSource::from(path.as_path());
                (path.to_string_lossy().to_string(), source)
            })
            .collect::<BTreeMap<String, SolcStandardJsonInputSource>>();
        Self::try_from_llvm_ir_sources(sources, libraries, solc_output)
    }

    /// Parses the LLVM IR `sources` and returns an LLVM IR project.
    pub fn try_from_llvm_ir_sources(
        sources: BTreeMap<String, SolcStandardJsonInputSource>,
        libraries: SolcStandardJsonInputSettingsLibraries,
        mut solc_output: Option<&mut SolcStandardJsonOutput>,
    ) -> anyhow::Result<Self> {
        let results = sources
            .into_par_iter()
            .map(|(path, mut source)| {
                let source_code = match source.try_resolve() {
                    Ok(()) => source.take_content().expect("Always exists"),
                    Err(error) => return (path, Err(error)),
                };

                let source_hash = revive_common::Keccak256::from_slice(source_code.as_bytes());

                let contract = Contract::new(
                    ContractIdentifier::new(path.clone(), None),
                    LLVMIR::new(path.clone(), source_code).into(),
                    serde_json::json!({
                        "source_hash": source_hash.to_string(),
                    }),
                );

                (path, Ok(contract))
            })
            .collect::<BTreeMap<String, anyhow::Result<Contract>>>();

        let mut contracts = BTreeMap::new();
        for (path, result) in results.into_iter() {
            match result {
                Ok(contract) => {
                    contracts.insert(path, contract);
                }
                Err(error) => match solc_output {
                    Some(ref mut solc_output) => solc_output.push_error(Some(path), error),
                    None => anyhow::bail!(error),
                },
            }
        }
        Ok(Self::new(None, contracts, libraries))
    }

    /// Converts the `solc` JSON output into a convenient project.
    pub fn try_from_standard_json_output(
        solc_output: &mut SolcStandardJsonOutput,
        libraries: SolcStandardJsonInputSettingsLibraries,
        solc_version: &SolcVersion,
        debug_config: &revive_llvm_context::DebugConfig,
    ) -> anyhow::Result<Self> {
        let mut input_contracts = Vec::with_capacity(solc_output.contracts.len());
        for (path, file) in solc_output.contracts.iter() {
            for (name, contract) in file.iter() {
                let name = ContractIdentifier::new((*path).to_owned(), Some((*name).to_owned()));
                input_contracts.push((name, contract));
            }
        }

        #[cfg(feature = "parallel")]
        let iter = input_contracts.into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = input_contracts.into_iter();

        let results = iter
            .filter_map(|(name, contract)| {
                let ir = match Yul::try_from_source(
                    name.full_path.as_str(),
                    contract.ir_optimized.as_str(),
                    debug_config,
                )
                .map(|yul| yul.map(IR::from))
                {
                    Ok(ir) => ir?,
                    Err(error) => return Some((name.full_path, Err(error))),
                };
                let contract = Contract::new(name.clone(), ir, contract.metadata.clone());
                Some((name.full_path, Ok(contract)))
            })
            .collect::<BTreeMap<String, anyhow::Result<Contract>>>();

        let mut contracts = BTreeMap::new();
        for (path, result) in results.into_iter() {
            match result {
                Ok(contract) => {
                    contracts.insert(path, contract);
                }
                Err(error) => solc_output.push_error(Some(path), error),
            }
        }
        Ok(Project::new(
            Some(solc_version.clone()),
            contracts,
            libraries,
        ))
    }
}
