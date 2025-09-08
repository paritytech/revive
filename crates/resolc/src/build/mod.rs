//! The Solidity project build.

pub mod contract;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use normpath::PathExt;

use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::CombinedJsonContract;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputContract;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use revive_solc_json_interface::SolcStandardJsonOutputErrorHandler;

use crate::build::contract::Contract;
use crate::solc::version::Version as SolcVersion;

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
    ) -> Self {
        let mut contracts: BTreeMap<String, Contract> = self
            .results
            .into_iter()
            .map(|(path, result)| (path, result.expect("Cannot link a project with errors")))
            .collect();
        todo!()
    }

    /// Writes all contracts to the specified directory.
    pub fn write_to_directory(
        mut self,
        output_directory: &Path,
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
