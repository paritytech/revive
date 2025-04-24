//! The LLVM context library.

pub mod r#const;
pub mod context;
pub mod evm;

pub use self::r#const::*;

use crate::debug_config::DebugConfig;
use crate::memory::MemoryConfig;
use crate::optimizer::settings::Settings as OptimizerSettings;

use anyhow::Context as AnyhowContext;
use polkavm_common::program::ProgramBlob;
use polkavm_disassembler::{Disassembler, DisassemblyFormat};
use sha3::Digest;

use self::context::build::Build;
use self::context::Context;

/// Builds PolkaVM assembly text.
pub fn build_assembly_text(
    contract_path: &str,
    bytecode: &[u8],
    metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
    debug_config: &DebugConfig,
) -> anyhow::Result<Build> {
    let program_blob = ProgramBlob::parse(bytecode.into())
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Failed to parse program blob for contract: {contract_path}"))?;

    let mut disassembler = Disassembler::new(&program_blob, DisassemblyFormat::Guest)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Failed to create disassembler for contract: {contract_path}"))?;
    disassembler.display_gas()?;

    let mut disassembled_code = Vec::new();
    disassembler
        .disassemble_into(&mut disassembled_code)
        .with_context(|| format!("Failed to disassemble contract: {}", contract_path))?;

    let assembly_text = String::from_utf8(disassembled_code).with_context(|| {
        format!("Failed to convert disassembled code to string for contract: {contract_path}")
    })?;

    debug_config.dump_assembly(contract_path, &assembly_text)?;

    Ok(Build::new(
        assembly_text.to_owned(),
        metadata_hash,
        bytecode.to_owned(),
        hex::encode(sha3::Keccak256::digest(bytecode)),
    ))
}

/// Implemented by items which are translated into LLVM IR.
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
        include_metadata_hash: bool,
        debug_config: DebugConfig,
        llvm_arguments: &[String],
        memory_config: MemoryConfig,
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
        _include_metadata_hash: bool,
        _debug_config: DebugConfig,
        _llvm_arguments: &[String],
        _memory_config: MemoryConfig,
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
