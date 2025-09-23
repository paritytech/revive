//! The Solidity contract build.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use revive_solc_json_interface::CombinedJsonContract;
use revive_solc_json_interface::SolcStandardJsonOutputContract;
use revive_solc_json_interface::SolcStandardJsonOutputContractEVM;
use serde::Deserialize;
use serde::Serialize;

/// The Solidity contract build.
#[derive(Debug, Serialize, Deserialize)]
pub struct Contract {
    /// The contract identifier.
    pub identifier: revive_common::ContractIdentifier,
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
        identifier: revive_common::ContractIdentifier,
        build: revive_llvm_context::PolkaVMBuild,
        metadata_json: serde_json::Value,
        missing_libraries: BTreeSet<String>,
        factory_dependencies: BTreeSet<String>,
        object_format: revive_common::ObjectFormat,
    ) -> Self {
        Self {
            identifier,
            build,
            metadata_json,
            missing_libraries,
            factory_dependencies,
            factory_dependencies_resolved: BTreeMap::new(),
            object_format,
        }
    }

    /// Writes the contract text assembly and bytecode to terminal.
    pub fn write_to_terminal(
        self,
        path: String,
        output_metadata: bool,
        output_assembly: bool,
        output_binary: bool,
    ) -> anyhow::Result<()> {
        writeln!(std::io::stdout(), "\n======= {path} =======")?;
        if output_assembly {
            writeln!(
                std::io::stdout(),
                "Assembly:\n{}",
                self.build.assembly_text.unwrap_or_default(),
            )?;
        }
        if output_metadata {
            writeln!(std::io::stdout(), "Metadata:\n{}", self.metadata_json)?;
        }
        if output_binary {
            writeln!(
                std::io::stdout(),
                "Binary:\n{}",
                hex::encode(self.build.bytecode)
            )?;
        }

        Ok(())
    }
    /// Writes the contract text assembly and bytecode to files.
    pub fn write_to_directory(
        self,
        path: &Path,
        output_metadata: bool,
        output_assembly: bool,
        output_binary: bool,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let file_path = PathBuf::from(self.identifier.path);
        let file_name = file_path
            .file_name()
            .expect("Always exists")
            .to_str()
            .expect("Always valid");

        let mut output_path = path.to_owned();
        output_path.push(file_name);
        std::fs::create_dir_all(output_path.as_path())?;

        if output_metadata {
            let output_name = format!(
                "{}_meta.{}",
                self.identifier.name.as_deref().unwrap_or(file_name),
                revive_common::EXTENSION_JSON,
            );
            let mut output_path = output_path.clone();
            output_path.push(output_name.as_str());

            if output_path.exists() && !overwrite {
                anyhow::bail!(
                    "Refusing to overwrite an existing file {output_path:?} (use --overwrite to force)."
                );
            } else {
                std::fs::write(
                    output_path.as_path(),
                    self.metadata_json.to_string().as_bytes(),
                )
                .map_err(|error| anyhow::anyhow!("File {output_path:?} writing: {error}"))?;
            }
        }
        if output_assembly {
            let file_name = format!("{file_name}.{}", revive_common::EXTENSION_POLKAVM_ASSEMBLY);
            let mut file_path = path.to_owned();
            file_path.push(file_name);

            if file_path.exists() && !overwrite {
                anyhow::bail!(
                    "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
                );
            } else {
                File::create(&file_path)
                    .map_err(|error| anyhow::anyhow!("File {file_path:?} creating error: {error}"))?
                    .write_all(self.build.assembly_text.unwrap_or_default().as_bytes())
                    .map_err(|error| {
                        anyhow::anyhow!("File {file_path:?} writing error: {error}")
                    })?;
            }
        }

        if output_binary {
            let file_name = format!("{file_name}.{}", revive_common::EXTENSION_POLKAVM_BINARY);
            let mut file_path = path.to_owned();
            file_path.push(file_name);

            if file_path.exists() && !overwrite {
                anyhow::bail!(
                    "Refusing to overwrite an existing file {file_path:?} (use --overwrite to force)."
                );
            } else {
                File::create(&file_path)
                    .map_err(|error| anyhow::anyhow!("File {file_path:?} creating error: {error}"))?
                    .write_all(self.build.bytecode.as_slice())
                    .map_err(|error| {
                        anyhow::anyhow!("File {file_path:?} writing error: {error}")
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
        let hexadecimal_bytecode = hex::encode(self.build.bytecode);

        if let Some(metadata) = combined_json_contract.metadata.as_mut() {
            *metadata = self.metadata_json.to_string();
        }

        combined_json_contract.assembly = self.build.assembly_text;
        combined_json_contract.bin = Some(hexadecimal_bytecode);
        combined_json_contract
            .bin_runtime
            .clone_from(&combined_json_contract.bin);

        combined_json_contract
            .missing_libraries
            .extend(self.missing_libraries);
        combined_json_contract
            .factory_deps_unlinked
            .extend(self.factory_dependencies);
        combined_json_contract.factory_deps.extend(
            self.factory_dependencies_resolved
                .into_iter()
                .map(|(hash, path)| (hex::encode(hash), path)),
        );
        combined_json_contract.object_format = Some(self.object_format);

        Ok(())
    }

    /// Writes the contract text assembly and bytecode to the standard JSON.
    pub fn write_to_standard_json(
        self,
        standard_json_contract: &mut SolcStandardJsonOutputContract,
    ) -> anyhow::Result<()> {
        let bytecode = hex::encode(self.build.bytecode.as_slice());
        let assembly_text = self.build.assembly_text.unwrap_or_default();

        standard_json_contract.metadata = self.metadata_json;
        standard_json_contract
            .evm
            .get_or_insert_with(Default::default)
            .modify(assembly_text, bytecode);
        standard_json_contract.hash = self.build.bytecode_hash.map(hex::encode);
        standard_json_contract
            .missing_libraries
            .extend(self.missing_libraries);
        standard_json_contract
            .factory_dependencies_unlinked
            .extend(self.factory_dependencies);
        standard_json_contract.factory_dependencies.extend(
            self.factory_dependencies_resolved
                .into_iter()
                .map(|(hash, path)| (hex::encode(hash), path)),
        );
        standard_json_contract.object_format = Some(self.object_format);

        Ok(())
    }

    /// Converts the full path to a short one.
    pub fn short_path(path: &str) -> &str {
        path.rfind('/')
            .map(|last_slash| &path[last_slash + 1..])
            .unwrap_or_else(|| path)
    }
}
