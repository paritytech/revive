//!
//! The `solc --asm-json` output.
//!

pub mod data;
pub mod instruction;

use std::collections::BTreeMap;
use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::evmla::ethereal_ir::entry_link::EntryLink;
use crate::evmla::ethereal_ir::EtherealIR;
use crate::solc::standard_json::output::contract::evm::extra_metadata::ExtraMetadata;

use self::data::Data;
use self::instruction::name::Name as InstructionName;
use self::instruction::Instruction;

///
/// The JSON assembly.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Assembly {
    /// The metadata string.
    #[serde(rename = ".auxdata")]
    pub auxdata: Option<String>,
    /// The deploy code instructions.
    #[serde(rename = ".code")]
    pub code: Option<Vec<Instruction>>,
    /// The runtime code.
    #[serde(rename = ".data")]
    pub data: Option<BTreeMap<String, Data>>,

    /// The full contract path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_path: Option<String>,
    /// The factory dependency paths.
    #[serde(default = "HashSet::new")]
    pub factory_dependencies: HashSet<String>,
    /// The EVMLA extra metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_metadata: Option<ExtraMetadata>,
}

impl Assembly {
    ///
    /// Gets the contract `keccak256` hash.
    ///
    pub fn keccak256(&self) -> String {
        let json = serde_json::to_vec(self).expect("Always valid");
        era_compiler_llvm_context::eravm_utils::keccak256(json.as_slice())
    }

    ///
    /// Sets the full contract path.
    ///
    pub fn set_full_path(&mut self, full_path: String) {
        self.full_path = Some(full_path);
    }

    ///
    /// Returns the full contract path if it is set, or `<undefined>` otherwise.
    ///
    /// # Panics
    /// If the `full_path` has not been set.
    ///
    pub fn full_path(&self) -> &str {
        self.full_path
            .as_deref()
            .unwrap_or_else(|| panic!("The full path of some contracts is unset"))
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut missing_libraries = HashSet::new();
        if let Some(code) = self.code.as_ref() {
            for instruction in code.iter() {
                if let InstructionName::PUSHLIB = instruction.name {
                    let library_path = instruction.value.to_owned().expect("Always exists");
                    missing_libraries.insert(library_path);
                }
            }
        }
        if let Some(data) = self.data.as_ref() {
            for (_, data) in data.iter() {
                missing_libraries.extend(data.get_missing_libraries());
            }
        }
        missing_libraries
    }

    ///
    /// Replaces the deploy code dependencies with full contract path and returns the list.
    ///
    pub fn deploy_dependencies_pass(
        &mut self,
        full_path: &str,
        hash_data_mapping: &BTreeMap<String, String>,
    ) -> anyhow::Result<BTreeMap<String, String>> {
        let mut index_path_mapping = BTreeMap::new();
        let index = "0".repeat(era_compiler_common::BYTE_LENGTH_FIELD * 2);
        index_path_mapping.insert(index, full_path.to_owned());

        let dependencies = match self.data.as_mut() {
            Some(dependencies) => dependencies,
            None => return Ok(index_path_mapping),
        };
        for (index, data) in dependencies.iter_mut() {
            if index == "0" {
                continue;
            }

            let mut index_extended =
                "0".repeat(era_compiler_common::BYTE_LENGTH_FIELD * 2 - index.len());
            index_extended.push_str(index.as_str());

            *data = match data {
                Data::Assembly(assembly) => {
                    let hash = assembly.keccak256();
                    let full_path =
                        hash_data_mapping
                            .get(hash.as_str())
                            .cloned()
                            .ok_or_else(|| {
                                anyhow::anyhow!("Contract path not found for hash `{}`", hash)
                            })?;
                    self.factory_dependencies.insert(full_path.to_owned());

                    index_path_mapping.insert(index_extended, full_path.clone());
                    Data::Path(full_path)
                }
                Data::Hash(hash) => {
                    index_path_mapping.insert(index_extended, hash.to_owned());
                    continue;
                }
                _ => continue,
            };
        }

        Ok(index_path_mapping)
    }

    ///
    /// Replaces the runtime code dependencies with full contract path and returns the list.
    ///
    pub fn runtime_dependencies_pass(
        &mut self,
        full_path: &str,
        hash_data_mapping: &BTreeMap<String, String>,
    ) -> anyhow::Result<BTreeMap<String, String>> {
        let mut index_path_mapping = BTreeMap::new();
        let index = "0".repeat(era_compiler_common::BYTE_LENGTH_FIELD * 2);
        index_path_mapping.insert(index, full_path.to_owned());

        let dependencies = match self
            .data
            .as_mut()
            .and_then(|data| data.get_mut("0"))
            .and_then(|data| data.get_assembly_mut())
            .and_then(|assembly| assembly.data.as_mut())
        {
            Some(dependencies) => dependencies,
            None => return Ok(index_path_mapping),
        };
        for (index, data) in dependencies.iter_mut() {
            let mut index_extended =
                "0".repeat(era_compiler_common::BYTE_LENGTH_FIELD * 2 - index.len());
            index_extended.push_str(index.as_str());

            *data = match data {
                Data::Assembly(assembly) => {
                    let hash = assembly.keccak256();
                    let full_path =
                        hash_data_mapping
                            .get(hash.as_str())
                            .cloned()
                            .ok_or_else(|| {
                                anyhow::anyhow!("Contract path not found for hash `{}`", hash)
                            })?;
                    self.factory_dependencies.insert(full_path.to_owned());

                    index_path_mapping.insert(index_extended, full_path.clone());
                    Data::Path(full_path)
                }
                Data::Hash(hash) => {
                    index_path_mapping.insert(index_extended, hash.to_owned());
                    continue;
                }
                _ => continue,
            };
        }

        Ok(index_path_mapping)
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for Assembly
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        let mut entry = era_compiler_llvm_context::EraVMEntryFunction::default();
        entry.declare(context)?;

        let mut runtime = era_compiler_llvm_context::EraVMRuntime::new(
            era_compiler_llvm_context::EraVMAddressSpace::Heap,
        );
        runtime.declare(context)?;

        era_compiler_llvm_context::EraVMDeployCodeFunction::new(
            era_compiler_llvm_context::EraVMDummyLLVMWritable::default(),
        )
        .declare(context)?;
        era_compiler_llvm_context::EraVMRuntimeCodeFunction::new(
            era_compiler_llvm_context::EraVMDummyLLVMWritable::default(),
        )
        .declare(context)?;

        entry.into_llvm(context)?;

        runtime.into_llvm(context)?;

        Ok(())
    }

    fn into_llvm(
        mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        let full_path = self.full_path().to_owned();

        if let Some(debug_config) = context.debug_config() {
            debug_config.dump_evmla(full_path.as_str(), self.to_string().as_str())?;
        }
        let deploy_code_blocks = EtherealIR::get_blocks(
            context.evmla().version.to_owned(),
            era_compiler_llvm_context::EraVMCodeType::Deploy,
            self.code
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Deploy code instructions not found"))?,
        )?;

        let data = self
            .data
            .ok_or_else(|| anyhow::anyhow!("Runtime code data not found"))?
            .remove("0")
            .expect("Always exists");
        if let Some(debug_config) = context.debug_config() {
            debug_config.dump_evmla(full_path.as_str(), data.to_string().as_str())?;
        }
        let runtime_code_instructions = match data {
            Data::Assembly(assembly) => assembly
                .code
                .ok_or_else(|| anyhow::anyhow!("Runtime code instructions not found"))?,
            Data::Hash(hash) => {
                anyhow::bail!("Expected runtime code instructions, found hash `{}`", hash)
            }
            Data::Path(path) => {
                anyhow::bail!("Expected runtime code instructions, found path `{}`", path)
            }
        };
        let runtime_code_blocks = EtherealIR::get_blocks(
            context.evmla().version.to_owned(),
            era_compiler_llvm_context::EraVMCodeType::Runtime,
            runtime_code_instructions.as_slice(),
        )?;

        let extra_metadata = self.extra_metadata.take().unwrap_or_default();
        let mut blocks = deploy_code_blocks;
        blocks.extend(runtime_code_blocks);
        let mut ethereal_ir =
            EtherealIR::new(context.evmla().version.to_owned(), extra_metadata, blocks)?;
        if let Some(debug_config) = context.debug_config() {
            debug_config.dump_ethir(full_path.as_str(), ethereal_ir.to_string().as_str())?;
        }
        ethereal_ir.declare(context)?;
        ethereal_ir.into_llvm(context)?;

        era_compiler_llvm_context::EraVMDeployCodeFunction::new(EntryLink::new(
            era_compiler_llvm_context::EraVMCodeType::Deploy,
        ))
        .into_llvm(context)?;
        era_compiler_llvm_context::EraVMRuntimeCodeFunction::new(EntryLink::new(
            era_compiler_llvm_context::EraVMCodeType::Runtime,
        ))
        .into_llvm(context)?;

        Ok(())
    }
}

impl std::fmt::Display for Assembly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(instructions) = self.code.as_ref() {
            for (index, instruction) in instructions.iter().enumerate() {
                match instruction.name {
                    InstructionName::Tag => writeln!(f, "{index:03} {instruction}")?,
                    _ => writeln!(f, "{index:03}     {instruction}")?,
                }
            }
        }

        Ok(())
    }
}
