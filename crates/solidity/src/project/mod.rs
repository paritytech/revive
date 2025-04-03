//! The processed input data.

pub mod contract;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

#[cfg(feature = "parallel")]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use revive_solc_json_interface::SolcStandardJsonOutput;
use serde::Deserialize;
use serde::Serialize;
use sha3::Digest;

use crate::build::contract::Contract as ContractBuild;
use crate::build::Build;
use crate::missing_libraries::MissingLibraries;
use crate::process::input::Input as ProcessInput;
use crate::process::Process;
use crate::project::contract::ir::IR;
use crate::solc::version::Version as SolcVersion;
use crate::solc::Compiler;
use crate::yul::lexer::Lexer;
use crate::yul::parser::statement::object::Object;

use self::contract::Contract;

/// The processes input data.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    /// The source code version.
    pub version: SolcVersion,
    /// The project contracts,
    pub contracts: BTreeMap<String, Contract>,
    /// The mapping of auxiliary identifiers, e.g. Yul object names, to full contract paths.
    pub identifier_paths: BTreeMap<String, String>,
    /// The library addresses.
    pub libraries: BTreeMap<String, BTreeMap<String, String>>,
}

impl Project {
    /// A shortcut constructor.
    pub fn new(
        version: SolcVersion,
        contracts: BTreeMap<String, Contract>,
        libraries: BTreeMap<String, BTreeMap<String, String>>,
    ) -> Self {
        let mut identifier_paths = BTreeMap::new();
        for (path, contract) in contracts.iter() {
            identifier_paths.insert(contract.identifier().to_owned(), path.to_owned());
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
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        include_metadata_hash: bool,
        debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: &[String],
    ) -> anyhow::Result<Build> {
        let project = self.clone();
        #[cfg(feature = "parallel")]
        let iter = self.contracts.into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = self.contracts.into_iter();

        let results: BTreeMap<String, anyhow::Result<ContractBuild>> = iter
            .map(|(full_path, contract)| {
                let process_input = ProcessInput::new(
                    contract,
                    project.clone(),
                    include_metadata_hash,
                    optimizer_settings.clone(),
                    debug_config.clone(),
                    llvm_arguments.to_vec(),
                );
                let process_output = {
                    #[cfg(target_os = "emscripten")]
                    {
                        crate::WorkerProcess::call(process_input)
                    }
                    #[cfg(not(target_os = "emscripten"))]
                    {
                        crate::NativeProcess::call(process_input)
                    }
                };
                (full_path, process_output.map(|output| output.build))
            })
            .collect();

        let mut build = Build::default();
        let mut hashes = HashMap::with_capacity(results.len());
        for (path, result) in results.iter() {
            match result {
                Ok(contract) => {
                    hashes.insert(path.to_owned(), contract.build.bytecode_hash.to_owned());
                }
                Err(error) => {
                    anyhow::bail!("Contract `{}` compiling error: {:?}", path, error);
                }
            }
        }
        for (path, result) in results.into_iter() {
            match result {
                Ok(mut contract) => {
                    for dependency in contract.factory_dependencies.drain() {
                        let dependency_path = project
                            .identifier_paths
                            .get(dependency.as_str())
                            .cloned()
                            .unwrap_or_else(|| {
                                panic!("Dependency `{dependency}` full path not found")
                            });
                        let hash = match hashes.get(dependency_path.as_str()) {
                            Some(hash) => hash.to_owned(),
                            None => anyhow::bail!(
                                "Dependency contract `{}` not found in the project",
                                dependency_path
                            ),
                        };
                        contract
                            .build
                            .factory_dependencies
                            .insert(hash, dependency_path);
                    }

                    build.contracts.insert(path, contract);
                }
                Err(error) => {
                    anyhow::bail!("Contract `{}` compiling error: {:?}", path, error);
                }
            }
        }

        Ok(build)
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> MissingLibraries {
        let deployed_libraries = self
            .libraries
            .iter()
            .flat_map(|(file, names)| {
                names
                    .iter()
                    .map(|(name, _address)| format!("{file}:{name}"))
                    .collect::<HashSet<String>>()
            })
            .collect::<HashSet<String>>();

        let mut missing_deployable_libraries = BTreeMap::new();
        for (contract_path, contract) in self.contracts.iter() {
            let missing_libraries = contract
                .get_missing_libraries()
                .into_iter()
                .filter(|library| !deployed_libraries.contains(library))
                .collect::<HashSet<String>>();
            missing_deployable_libraries.insert(contract_path.to_owned(), missing_libraries);
        }
        MissingLibraries::new(missing_deployable_libraries)
    }

    /// Parses the Yul source code file and returns the source data.
    pub fn try_from_yul_path<T: Compiler>(
        path: &Path,
        solc_validator: Option<&T>,
    ) -> anyhow::Result<Self> {
        let source_code = std::fs::read_to_string(path)
            .map_err(|error| anyhow::anyhow!("Yul file {:?} reading error: {}", path, error))?;
        Self::try_from_yul_string(path, source_code.as_str(), solc_validator)
    }

    /// Parses the test Yul source code string and returns the source data.
    /// Only for integration testing purposes.
    pub fn try_from_yul_string<T: Compiler>(
        path: &Path,
        source_code: &str,
        solc_validator: Option<&T>,
    ) -> anyhow::Result<Self> {
        if let Some(solc) = solc_validator {
            solc.validate_yul(path)?;
        }

        let source_version = SolcVersion::new_simple(crate::solc::LAST_SUPPORTED_VERSION);
        let path = path.to_string_lossy().to_string();
        let source_hash = sha3::Keccak256::digest(source_code.as_bytes()).into();

        let mut lexer = Lexer::new(source_code.to_owned());
        let object = Object::parse(&mut lexer, None)
            .map_err(|error| anyhow::anyhow!("Yul object `{}` parsing error: {}", path, error))?;

        let mut project_contracts = BTreeMap::new();
        project_contracts.insert(
            path.to_owned(),
            Contract::new(
                path,
                source_hash,
                source_version.clone(),
                IR::new_yul(source_code.to_owned(), object),
                None,
            ),
        );

        Ok(Self::new(
            source_version,
            project_contracts,
            BTreeMap::new(),
        ))
    }

    /// Parses the LLVM IR source code file and returns the source data.
    pub fn try_from_llvm_ir_path(path: &Path) -> anyhow::Result<Self> {
        let source_code = std::fs::read_to_string(path)
            .map_err(|error| anyhow::anyhow!("LLVM IR file {:?} reading error: {}", path, error))?;
        let source_hash = sha3::Keccak256::digest(source_code.as_bytes()).into();

        let source_version =
            SolcVersion::new_simple(revive_llvm_context::polkavm_const::LLVM_VERSION);
        let path = path.to_string_lossy().to_string();

        let mut project_contracts = BTreeMap::new();
        project_contracts.insert(
            path.clone(),
            Contract::new(
                path.clone(),
                source_hash,
                source_version.clone(),
                IR::new_llvm_ir(path, source_code),
                None,
            ),
        );

        Ok(Self::new(
            source_version,
            project_contracts,
            BTreeMap::new(),
        ))
    }

    /// Converts the `solc` JSON output into a convenient project.
    pub fn try_from_standard_json_output(
        output: &SolcStandardJsonOutput,
        source_code_files: BTreeMap<String, String>,
        libraries: BTreeMap<String, BTreeMap<String, String>>,
        solc_version: &SolcVersion,
        debug_config: &revive_llvm_context::DebugConfig,
    ) -> anyhow::Result<Self> {
        let files = match output.contracts.as_ref() {
            Some(files) => files,
            None => match &output.errors {
                Some(errors) if errors.iter().any(|e| e.severity == "error") => {
                    anyhow::bail!(serde_json::to_string_pretty(errors).expect("Always valid"));
                }
                _ => &BTreeMap::new(),
            },
        };
        let mut project_contracts = BTreeMap::new();

        for (path, contracts) in files.iter() {
            for (name, contract) in contracts.iter() {
                let full_path = format!("{path}:{name}");

                let ir_optimized = match contract.ir_optimized.to_owned() {
                    Some(ir_optimized) => ir_optimized,
                    None => continue,
                };
                if ir_optimized.is_empty() {
                    continue;
                }

                debug_config.dump_yul(full_path.as_str(), ir_optimized.as_str())?;

                let mut lexer = Lexer::new(ir_optimized.to_owned());
                let object = Object::parse(&mut lexer, None).map_err(|error| {
                    anyhow::anyhow!("Contract `{}` parsing error: {:?}", full_path, error)
                })?;

                let source = IR::new_yul(ir_optimized.to_owned(), object);

                let source_code = source_code_files
                    .get(path.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Source code for path `{}` not found", path))?;
                let source_hash = sha3::Keccak256::digest(source_code.as_bytes()).into();

                let project_contract = Contract::new(
                    full_path.clone(),
                    source_hash,
                    solc_version.to_owned(),
                    source,
                    contract.metadata.to_owned(),
                );
                project_contracts.insert(full_path, project_contract);
            }
        }

        Ok(Project::new(
            solc_version.to_owned(),
            project_contracts,
            libraries,
        ))
    }
}

impl revive_llvm_context::PolkaVMDependency for Project {
    fn compile(
        project: Self,
        identifier: &str,
        optimizer_settings: revive_llvm_context::OptimizerSettings,
        include_metadata_hash: bool,
        debug_config: revive_llvm_context::DebugConfig,
        llvm_arguments: &[String],
    ) -> anyhow::Result<String> {
        let contract_path = project.resolve_path(identifier)?;
        let contract = project
            .contracts
            .get(contract_path.as_str())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Dependency contract `{}` not found in the project",
                    contract_path
                )
            })?;

        contract
            .compile(
                project,
                optimizer_settings,
                include_metadata_hash,
                debug_config,
                llvm_arguments,
            )
            .map_err(|error| {
                anyhow::anyhow!(
                    "Dependency contract `{}` compiling error: {}",
                    identifier,
                    error
                )
            })
            .map(|contract| contract.build.bytecode_hash)
    }

    fn resolve_path(&self, identifier: &str) -> anyhow::Result<String> {
        self.identifier_paths
            .get(identifier.strip_suffix("_deployed").unwrap_or(identifier))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Contract with identifier `{}` not found in the project",
                    identifier
                )
            })
    }

    fn resolve_library(&self, path: &str) -> anyhow::Result<String> {
        for (file_path, contracts) in self.libraries.iter() {
            for (contract_name, address) in contracts.iter() {
                let key = format!("{file_path}:{contract_name}");
                if key.as_str() == path {
                    return Ok(address["0x".len()..].to_owned());
                }
            }
        }

        anyhow::bail!("Library `{}` not found in the project", path);
    }
}
