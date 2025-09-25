//! The Solidity project build.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use normpath::PathExt;

use revive_common::ObjectFormat;
use revive_llvm_context::DebugConfig;
use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::CombinedJsonContract;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputContract;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use revive_solc_json_interface::SolcStandardJsonOutputErrorHandler;

use crate::build::contract::Contract;
use crate::solc::version::Version as SolcVersion;

pub mod contract;

/// The Solidity project PVM build.
#[derive(Debug, Default)]
pub struct Build {
    /// The contract data,
    pub results: BTreeMap<String, Result<Contract, SolcStandardJsonOutputError>>,
    /// The additional message to output (added by the revive compiler).
    pub messages: Vec<revive_solc_json_interface::SolcStandardJsonOutputError>,
}

impl Build {
    /// A shorthand constructor.
    ///
    /// Note: Takes the supplied `messages`, leaving an empty vec.
    pub fn new(
        results: BTreeMap<String, Result<Contract, SolcStandardJsonOutputError>>,
        messages: &mut Vec<revive_solc_json_interface::SolcStandardJsonOutputError>,
    ) -> Self {
        Self {
            results,
            messages: std::mem::take(messages),
        }
    }

    /// Links the PVM build.
    pub fn link(
        mut self,
        linker_symbols: BTreeMap<String, [u8; revive_common::BYTE_LENGTH_ETH_ADDRESS]>,
        debug_config: &DebugConfig,
    ) -> Self {
        let mut contracts: BTreeMap<String, Contract> = self
            .results
            .into_iter()
            .map(|(path, result)| (path, result.expect("Cannot link a project with errors")))
            .collect();

        loop {
            let mut linkage_data = BTreeMap::new();
            for (path, contract) in contracts
                .iter()
                .filter(|(_path, contract)| contract.object_format == ObjectFormat::ELF)
            {
                match revive_llvm_context::polkavm_link(
                    &contract.build.bytecode,
                    &linker_symbols,
                    &contract
                        .factory_dependencies
                        .iter()
                        .filter_map(|dependency| {
                            let bytecode_hash = contracts
                                .get(dependency)
                                .as_ref()?
                                .build
                                .bytecode_hash
                                .as_ref()?
                                .to_owned();
                            Some((dependency.to_owned(), bytecode_hash))
                        })
                        .collect(),
                ) {
                    Ok((memory_buffer_linked, ObjectFormat::PVM)) => {
                        let bytecode_hash =
                            revive_llvm_context::polkavm_hash(&memory_buffer_linked);
                        let assembly_text = revive_llvm_context::polkavm_disassemble(
                            path,
                            &memory_buffer_linked,
                            debug_config,
                        )
                        .unwrap_or_else(|error| {
                            panic!("ICE: The PVM disassembler failed: {error}")
                        });
                        linkage_data.insert(
                            path.to_owned(),
                            (memory_buffer_linked, bytecode_hash, assembly_text),
                        );
                    }
                    Ok((_memory_buffer_linked, ObjectFormat::ELF)) => {}
                    Err(error) => self
                        .messages
                        .push(SolcStandardJsonOutputError::new_error(error, None, None)),
                }
            }
            if linkage_data.is_empty() {
                break;
            }

            for (path, (memory_buffer_linked, bytecode_hash, assembly_text)) in
                linkage_data.into_iter()
            {
                let contract = contracts.get(path.as_str()).expect("Always exists");
                let factory_dependencies_resolved = contract
                    .factory_dependencies
                    .iter()
                    .filter_map(|dependency| {
                        Some((
                            contracts
                                .get(dependency)
                                .as_ref()?
                                .build
                                .bytecode_hash
                                .as_ref()?
                                .to_owned(),
                            dependency.to_owned(),
                        ))
                    })
                    .collect();
                let contract = contracts.get_mut(path.as_str()).expect("Always exists");
                contract.build.bytecode = memory_buffer_linked.as_slice().to_vec();
                contract.build.bytecode_hash = Some(bytecode_hash);
                contract.build.assembly_text = Some(assembly_text);
                contract.factory_dependencies_resolved = factory_dependencies_resolved;
                contract.object_format = revive_common::ObjectFormat::PVM;
            }
        }

        Self::new(
            contracts
                .into_iter()
                .map(|(path, contract)| (path, Ok(contract)))
                .collect(),
            &mut self.messages,
        )
    }

    /// Writes all contracts to the terminal.
    pub fn write_to_terminal(
        mut self,
        output_metadata: bool,
        output_assembly: bool,
        output_binary: bool,
    ) -> anyhow::Result<()> {
        self.take_and_write_warnings();
        self.exit_on_error();

        if !output_metadata && !output_assembly && !output_binary {
            writeln!(
                std::io::stderr(),
                "Compiler run successful. No output requested. Use flags --metadata, --asm, --bin."
            )?;
            return Ok(());
        }

        for (path, build) in self.results.into_iter() {
            build.expect("Always valid").write_to_terminal(
                path,
                output_metadata,
                output_assembly,
                output_binary,
            )?;
        }

        Ok(())
    }

    /// Writes all contracts to the specified directory.
    pub fn write_to_directory(
        mut self,
        output_directory: &Path,
        output_metadata: bool,
        output_assembly: bool,
        output_binary: bool,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        self.take_and_write_warnings();
        self.exit_on_error();

        std::fs::create_dir_all(output_directory)?;

        for build in self.results.into_values() {
            build.expect("Always valid").write_to_directory(
                output_directory,
                output_metadata,
                output_assembly,
                output_binary,
                overwrite,
            )?;
        }

        writeln!(
            std::io::stderr(),
            "Compiler run successful. Artifact(s) can be found in directory {output_directory:?}."
        )?;
        Ok(())
    }

    /// Writes all contracts assembly and bytecode to the combined JSON.
    pub fn write_to_combined_json(
        mut self,
        combined_json: &mut CombinedJson,
    ) -> anyhow::Result<()> {
        self.take_and_write_warnings();
        self.exit_on_error();

        for result in self.results.into_values() {
            let build = result.expect("Exits on an error above");
            let identifier = build.identifier.clone();

            let combined_json_contract =
                match combined_json
                    .contracts
                    .iter_mut()
                    .find_map(|(json_path, contract)| {
                        if Self::normalize_full_path(identifier.full_path.as_str())
                            .ends_with(Self::normalize_full_path(json_path).as_str())
                        {
                            Some(contract)
                        } else {
                            None
                        }
                    }) {
                    Some(contract) => contract,
                    None => {
                        combined_json.contracts.insert(
                            identifier.full_path.clone(),
                            CombinedJsonContract::default(),
                        );
                        combined_json
                            .contracts
                            .get_mut(identifier.full_path.as_str())
                            .expect("Always exists")
                    }
                };

            build.write_to_combined_json(combined_json_contract)?;
        }

        Ok(())
    }

    /// Writes all contracts assembly and bytecode to the standard JSON.
    pub fn write_to_standard_json(
        self,
        standard_json: &mut SolcStandardJsonOutput,
        solc_version: &SolcVersion,
    ) -> anyhow::Result<()> {
        let mut errors = Vec::with_capacity(self.results.len());
        for result in self.results.into_values() {
            let build = match result {
                Ok(build) => build,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let identifier = build.identifier.clone();

            match standard_json
                .contracts
                .get_mut(identifier.path.as_str())
                .and_then(|contracts| {
                    contracts.get_mut(
                        identifier
                            .name
                            .as_deref()
                            .unwrap_or(identifier.path.as_str()),
                    )
                }) {
                Some(contract) => {
                    build.write_to_standard_json(contract)?;
                }
                None => {
                    let contracts = standard_json
                        .contracts
                        .entry(identifier.path.clone())
                        .or_default();
                    let mut contract = SolcStandardJsonOutputContract::default();
                    build.write_to_standard_json(&mut contract)?;
                    contracts.insert(identifier.name.unwrap_or(identifier.path), contract);
                }
            }
        }

        standard_json.errors.extend(errors);
        standard_json.version = Some(solc_version.default.to_string());
        standard_json.long_version = Some(solc_version.long.to_owned());

        Ok(())
    }

    /// Normalizes the full contract path.
    ///
    /// # Panics
    /// If the path does not contain a colon.
    fn normalize_full_path(path: &str) -> String {
        let mut iterator = path.split(':');
        let path = iterator.next().expect("Always exists");
        let name = iterator.next().expect("Always exists");

        let mut full_path = PathBuf::from(path)
            .normalize()
            .expect("Path normalization error")
            .as_os_str()
            .to_string_lossy()
            .into_owned();
        full_path.push(':');
        full_path.push_str(name);
        full_path
    }
}

impl revive_solc_json_interface::SolcStandardJsonOutputErrorHandler for Build {
    fn errors(&self) -> Vec<&revive_solc_json_interface::SolcStandardJsonOutputError> {
        let mut errors: Vec<&revive_solc_json_interface::SolcStandardJsonOutputError> = self
            .results
            .values()
            .filter_map(|build| build.as_ref().err())
            .collect();
        errors.extend(
            self.messages
                .iter()
                .filter(|message| message.severity == "error"),
        );
        errors
    }

    fn take_warnings(&mut self) -> Vec<revive_solc_json_interface::SolcStandardJsonOutputError> {
        let warnings = self
            .messages
            .iter()
            .filter(|message| message.severity == "warning")
            .cloned()
            .collect();
        self.messages
            .retain(|message| message.severity != "warning");
        warnings
    }
}
