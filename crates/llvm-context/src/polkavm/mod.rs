//! The LLVM context library.

pub mod r#const;
pub mod context;
pub mod evm;
pub mod metadata_hash;
pub mod utils;

pub use self::r#const::*;

use crate::debug_config::DebugConfig;
use crate::optimizer::settings::Settings as OptimizerSettings;

use polkavm_common::program::ProgramBlob;
use polkavm_disassembler::{Disassembler, DisassemblyFormat};

use self::context::build::Build;
use self::context::Context;

/// Initializes the PolkaVM target machine.
pub fn initialize_target() {
    inkwell::targets::Target::initialize_riscv(&Default::default());
}

/// Builds PolkaVM assembly text.
pub fn build_assembly_text(
    contract_path: &str,
    encoded_hex_text: &str,
    _metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
    debug_config: Option<&DebugConfig>,
) -> anyhow::Result<Build> {
    if let Some(debug_config) = debug_config {
        debug_config.dump_assembly(contract_path, encoded_hex_text)?;
    }

    let bytecode = hex::decode(encoded_hex_text)
        .map_err(|e| anyhow::anyhow!("Failed to decode encoded hex text:\n{}\n", e))?;

    let program_blob = ProgramBlob::parse(bytecode.as_slice())
        .map_err(|error| anyhow::anyhow!(format!("Failed to parse program blob:\n{}\n", error)))?;

    let mut disassembler =
        Disassembler::new(&program_blob, DisassemblyFormat::Guest).map_err(|error| {
            anyhow::anyhow!(format!(
                "Failed to create disassembler for contract:\n{:?}\n\nDue to:\n{}\n",
                contract_path, error
            ))
        })?;

    let mut disassembled_code = Vec::new();
    disassembler
        .disassemble_into(&mut disassembled_code)
        .map_err(|error| {
            anyhow::anyhow!(format!(
                "Failed to disassemble contract:\n{:?}\n\nDue to:\n{}\n\nGas details:{:?}\n",
                contract_path,
                error,
                disassembler.display_gas()
            ))
        })?;

    let assembly_text = String::from_utf8(disassembled_code).map_err(|error| {
        anyhow::anyhow!(format!(
            "Failed to convert disassembled code to string for contract\n{:?}\n\nDue to:\n{}\n",
            contract_path, error
        ))
    })?;

    Ok(Build::new(
        assembly_text.to_owned(),
        Default::default(),
        bytecode.to_owned(),
        Default::default(),
    ))
}

/// Implemented by items which are translated into LLVM IR.
#[allow(clippy::upper_case_acronyms)]
pub trait WriteLLVM<D>
where
    D: Dependency + Clone,
{
    /// Declares the entity in the LLVM IR.
    /// Is usually performed in order to use the item before defining it.
    fn declare(&mut self, _context: &mut Context<D>) -> anyhow::Result<()> {
        Ok(())
    }

    /// Translates the entity into LLVM IR.
    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()>;
}

/// The dummy LLVM writable entity.
#[derive(Debug, Default, Clone)]
pub struct DummyLLVMWritable {}

impl<D> WriteLLVM<D> for DummyLLVMWritable
where
    D: Dependency + Clone,
{
    fn into_llvm(self, _context: &mut Context<D>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Implemented by items managing project dependencies.
pub trait Dependency {
    /// Compiles a project dependency.
    fn compile(
        dependency: Self,
        path: &str,
        optimizer_settings: OptimizerSettings,
        is_system_mode: bool,
        include_metadata_hash: bool,
        debug_config: Option<DebugConfig>,
    ) -> anyhow::Result<String>;

    /// Resolves a full contract path.
    fn resolve_path(&self, identifier: &str) -> anyhow::Result<String>;

    /// Resolves a library address.
    fn resolve_library(&self, path: &str) -> anyhow::Result<String>;
}

/// The dummy dependency entity.
#[derive(Debug, Default, Clone)]
pub struct DummyDependency {}

impl Dependency for DummyDependency {
    fn compile(
        _dependency: Self,
        _path: &str,
        _optimizer_settings: OptimizerSettings,
        _is_system_mode: bool,
        _include_metadata_hash: bool,
        _debug_config: Option<DebugConfig>,
    ) -> anyhow::Result<String> {
        Ok(String::new())
    }

    /// Resolves a full contract path.
    fn resolve_path(&self, _identifier: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }

    /// Resolves a library address.
    fn resolve_library(&self, _path: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}
