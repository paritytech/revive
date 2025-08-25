//! The Solidity contract build.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use revive_solc_json_interface::CombinedJsonContract;
use revive_solc_json_interface::SolcStandardJsonOutputContract;
use serde::Deserialize;
use serde::Serialize;

/// The Solidity contract build.
#[derive(Debug, Serialize, Deserialize)]
pub struct Contract {
    /// The contract path.
    pub path: String,
    /// The auxiliary identifier. Used to identify Yul objects.
    pub identifier: String,
    /// The LLVM module build.
    pub build: revive_llvm_context::PolkaVMBuild,
    /// The metadata JSON.
    pub metadata_json: serde_json::Value,
    /// The unlinked missing libraries.
    pub missing_libraries: BTreeSet<String>,
    /// The unresolved factory dependencies.
    pub factory_dependencies: BTreeSet<String>,
    /// The resolved factory dependencies.
    pub factory_dependencies_resolved: BTreeMap<[u8; revive_common::BYTE_LENGTH_WORD], String>,
    /// The binary object format.
    pub object_format: revive_common::ObjectFormat,
}

impl Contract {
    /// A shortcut constructor.
    pub fn new(
        path: String,
        identifier: String,
        build: revive_llvm_context::PolkaVMBuild,
        metadata_json: serde_json::Value,
        missing_libraries: BTreeSet<String>,
        factory_dependencies: BTreeSet<String>,
        object_format: revive_common::ObjectFormat,
    ) -> Self {
        Self {
            path,
            identifier,
            build,
            metadata_json,
            missing_libraries,
            factory_dependencies,
            factory_dependencies_resolved: BTreeMap::new(),
            object_format,
        }
    }

    /// Writes the contract text assembly and bytecode to files.
    pub fn write_to_directory(
        self,
        path: &Path,
        output_assembly: bool,
        output_binary: bool,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let file_name = Self::short_path(self.path.as_str());

        if output_assembly {
            let file_name = format!(
                "{}.{}",
                file_name,
                revive_common::EXTENSION_POLKAVM_ASSEMBLY
            );
            let mut file_path = path.to_owned();
            file_path.push(file_name);

            if file_path.exists() && !overwrite {
                anyhow::bail!(
                    "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
                );
            } else {
                let assembly_text = self.build.assembly_text;

                File::create(&file_path)
                    .map_err(|error| {
                        anyhow::anyhow!("File {:?} creating error: {}", file_path, error)
                    })?
                    .write_all(assembly_text.as_bytes())
                    .map_err(|error| {
                        anyhow::anyhow!("File {:?} writing error: {}", file_path, error)
                    })?;
            }
        }

        if output_binary {
            let file_name = format!("{}.{}", file_name, revive_common::EXTENSION_POLKAVM_BINARY);
            let mut file_path = path.to_owned();
            file_path.push(file_name);

            if file_path.exists() && !overwrite {
                anyhow::bail!(
                    "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
                );
            } else {
                File::create(&file_path)
                    .map_err(|error| {
                        anyhow::anyhow!("File {:?} creating error: {}", file_path, error)
                    })?
                    .write_all(self.build.bytecode.as_slice())
                    .map_err(|error| {
                        anyhow::anyhow!("File {:?} writing error: {}", file_path, error)
                    })?;
            }
        }

        Ok(())
    }

    /// Writes the contract text assembly and bytecode to the combined JSON.
    pub fn write_to_combined_json(
        self,
        combined_json_contract: &mut CombinedJsonContract,
    ) -> anyhow::Result<()> {
        if let Some(metadata) = combined_json_contract.metadata.as_mut() {
            *metadata = self.metadata_json.to_string();
        }

        if let Some(asm) = combined_json_contract.asm.as_mut() {
            *asm = serde_json::Value::String(self.build.assembly_text);
        }
        let hexadecimal_bytecode = hex::encode(self.build.bytecode);
        combined_json_contract.bin = Some(hexadecimal_bytecode);
        combined_json_contract
            .bin_runtime
            .clone_from(&combined_json_contract.bin);

        combined_json_contract.factory_deps = Some(self.build.factory_dependencies);

        Ok(())
    }

    /// Writes the contract text assembly and bytecode to the standard JSON.
    pub fn write_to_standard_json(
        self,
        standard_json_contract: &mut SolcStandardJsonOutputContract,
    ) -> anyhow::Result<()> {
        standard_json_contract.metadata = Some(self.metadata_json);

        let assembly_text = self.build.assembly_text;
        let bytecode = hex::encode(self.build.bytecode.as_slice());
        if let Some(evm) = standard_json_contract.evm.as_mut() {
            evm.modify(assembly_text, bytecode);
        }

        standard_json_contract.factory_dependencies = Some(self.build.factory_dependencies);
        standard_json_contract.hash = self.build.bytecode_hash.map(hex::encode);

        Ok(())
    }

    /// Converts the full path to a short one.
    pub fn short_path(path: &str) -> &str {
        path.rfind('/')
            .map(|last_slash| &path[last_slash + 1..])
            .unwrap_or_else(|| path)
    }
}
