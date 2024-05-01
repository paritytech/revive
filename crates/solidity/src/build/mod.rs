//! The Solidity project build.

pub mod contract;

use std::collections::BTreeMap;
use std::path::Path;

use crate::solc::combined_json::CombinedJson;
use crate::solc::standard_json::output::Output as StandardJsonOutput;
use crate::solc::version::Version as SolcVersion;

use self::contract::Contract;

/// The Solidity project build.
#[derive(Debug, Default)]
pub struct Build {
    /// The contract data,
    pub contracts: BTreeMap<String, Contract>,
}

impl Build {
    /// Writes all contracts to the specified directory.
    pub fn write_to_directory(
        self,
        output_directory: &Path,
        output_assembly: bool,
        output_binary: bool,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        for (_path, contract) in self.contracts.into_iter() {
            contract.write_to_directory(
                output_directory,
                output_assembly,
                output_binary,
                overwrite,
            )?;
        }

        Ok(())
    }

    /// Writes all contracts assembly and bytecode to the combined JSON.
    pub fn write_to_combined_json(
        self,
        combined_json: &mut CombinedJson,
        zksolc_version: &semver::Version,
    ) -> anyhow::Result<()> {
        for (path, contract) in self.contracts.into_iter() {
            let combined_json_contract = combined_json
                .contracts
                .iter_mut()
                .find_map(|(json_path, contract)| {
                    if path.ends_with(json_path) {
                        Some(contract)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("Contract `{}` not found in the project", path))?;

            contract.write_to_combined_json(combined_json_contract)?;
        }

        combined_json.zk_version = Some(zksolc_version.to_string());

        Ok(())
    }

    /// Writes all contracts assembly and bytecode to the standard JSON.
    pub fn write_to_standard_json(
        mut self,
        standard_json: &mut StandardJsonOutput,
        solc_version: &SolcVersion,
        zksolc_version: &semver::Version,
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
        standard_json.zk_version = Some(zksolc_version.to_string());

        Ok(())
    }
}
