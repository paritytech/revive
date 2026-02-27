//! The missing Solidity libraries.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use revive_solc_json_interface::SolcStandardJsonOutput;

use crate::solc::version::Version as SolcVersion;

/// The missing Solidity libraries.
pub struct MissingLibraries {
    /// The missing libraries.
    pub contract_libraries: BTreeMap<String, BTreeSet<String>>,
}

impl MissingLibraries {
    /// A shortcut constructor.
    pub fn new(contract_libraries: BTreeMap<String, BTreeSet<String>>) -> Self {
        Self { contract_libraries }
    }

    /// Writes the missing libraries to the standard JSON.
    pub fn write_to_standard_json(
        mut self,
        standard_json: &mut SolcStandardJsonOutput,
        solc_version: &SolcVersion,
    ) {
        for (path, file) in standard_json.contracts.iter_mut() {
            for (name, contract) in file.iter_mut() {
                let full_name = format!("{path}:{name}");
                let missing_libraries = self.contract_libraries.remove(full_name.as_str());

                if let Some(missing_libraries) = missing_libraries {
                    contract.missing_libraries = missing_libraries;
                }
            }
        }

        standard_json.version = Some(solc_version.default.to_string());
        standard_json.long_version = Some(solc_version.long.to_owned());
    }
}
