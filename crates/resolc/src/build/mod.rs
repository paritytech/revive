//! The Solidity project build.

pub mod contract;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use revive_solc_json_interface::combined_json::CombinedJson;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use revive_solc_json_interface::SolcStandardJsonOutputErrorHandler;

use crate::solc::version::Version as SolcVersion;
use crate::ResolcVersion;

use self::contract::Contract;

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
    pub fn write_to_combined_json(self, combined_json: &mut CombinedJson) -> anyhow::Result<()> {
        self.take_and_write_warnings();
        self.exit_on_error();

        for result in self.results.into_values() {
            let build = result.expect("Exits on an error above");
            let name = build.name.clone();

            let combined_json_contract =
                match combined_json
                    .contracts
                    .iter_mut()
                    .find_map(|(json_path, contract)| {
                        if Self::normalize_full_path(name.full_path.as_str())
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
                            name.full_path.clone(),
                            era_solc::CombinedJsonContract::default(),
                        );
                        combined_json
                            .contracts
                            .get_mut(name.full_path.as_str())
                            .expect("Always exists")
                    }
                };

            build.write_to_combined_json(combined_json_contract)?;
        }

        Ok(())
    }

    /// Writes all contracts assembly and bytecode to the standard JSON.
    pub fn write_to_standard_json(
        mut self,
        standard_json: &mut SolcStandardJsonOutput,
        solc_version: &SolcVersion,
    ) -> anyhow::Result<()> {
        let contracts = match standard_json.contracts.as_mut() {
            Some(contracts) => contracts,
            None => return Ok(()),
        };

        for (path, contracts) in contracts.iter_mut() {
            for (name, contract) in contracts.iter_mut() {
                let full_name = format!("{path}:{name}");

                if let Some(contract_data) = self.contracts.remove(full_name.as_str()) {
                    contract_data.write_to_standard_json(contract)?;
                }
            }
        }

        standard_json.version = Some(solc_version.default.to_string());
        standard_json.long_version = Some(solc_version.long.to_owned());
        standard_json.revive_version = Some(ResolcVersion::default().long);

        Ok(())
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
