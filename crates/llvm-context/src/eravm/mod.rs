//!
//! The LLVM context library.
//!

pub mod r#const;
pub mod context;
pub mod evm;
pub mod metadata_hash;
pub mod utils;

pub use self::r#const::*;

use crate::debug_config::DebugConfig;
use crate::optimizer::settings::Settings as OptimizerSettings;

use self::context::build::Build;
use self::context::Context;

///
/// Initializes the EraVM target machine.
///
pub fn initialize_target() {
    inkwell::targets::Target::initialize_riscv(&Default::default());
}

///
/// Builds EraVM assembly text.
///
pub fn build_assembly_text(
    contract_path: &str,
    assembly_text: &str,
    _metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_FIELD]>,
    debug_config: Option<&DebugConfig>,
) -> anyhow::Result<Build> {
    if let Some(debug_config) = debug_config {
        debug_config.dump_assembly(contract_path, assembly_text)?;
    }

    /*
    let mut assembly =
        zkevm_assembly::Assembly::from_string(assembly_text.to_owned(), metadata_hash).map_err(
            |error| {
                anyhow::anyhow!(
                    "The contract `{}` assembly parsing error: {}",
                    contract_path,
                    error,
                )
            },
        )?;

    let bytecode_words = match zkevm_assembly::get_encoding_mode() {
        zkevm_assembly::RunningVmEncodingMode::Production => { assembly.compile_to_bytecode_for_mode::<8, zkevm_opcode_defs::decoding::EncodingModeProduction>() },
        zkevm_assembly::RunningVmEncodingMode::Testing => { assembly.compile_to_bytecode_for_mode::<16, zkevm_opcode_defs::decoding::EncodingModeTesting>() },
    }
        .map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` assembly-to-bytecode conversion error: {}",
                contract_path,
                error,
            )
        })?;

    let bytecode_hash = match zkevm_assembly::get_encoding_mode() {
        zkevm_assembly::RunningVmEncodingMode::Production => {
            zkevm_opcode_defs::utils::bytecode_to_code_hash_for_mode::<
                8,
                zkevm_opcode_defs::decoding::EncodingModeProduction,
            >(bytecode_words.as_slice())
        }
        zkevm_assembly::RunningVmEncodingMode::Testing => {
            zkevm_opcode_defs::utils::bytecode_to_code_hash_for_mode::<
                16,
                zkevm_opcode_defs::decoding::EncodingModeTesting,
            >(bytecode_words.as_slice())
        }
    }
    .map(hex::encode)
    .map_err(|_error| {
        anyhow::anyhow!("The contract `{}` bytecode hashing error", contract_path,)
    })?;

    let bytecode = bytecode_words.into_iter().flatten().collect();
    */

    Ok(Build::new(
        assembly_text.to_owned(),
        Default::default(),
        hex::decode(assembly_text).unwrap(),
        Default::default(),
    ))
}

///
/// Implemented by items which are translated into LLVM IR.
///
#[allow(clippy::upper_case_acronyms)]
pub trait WriteLLVM<D>
where
    D: Dependency + Clone,
{
    ///
    /// Declares the entity in the LLVM IR.
    /// Is usually performed in order to use the item before defining it.
    ///
    fn declare(&mut self, _context: &mut Context<D>) -> anyhow::Result<()> {
        Ok(())
    }

    ///
    /// Translates the entity into LLVM IR.
    ///
    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()>;
}

///
/// The dummy LLVM writable entity.
///
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

///
/// Implemented by items managing project dependencies.
///
pub trait Dependency {
    ///
    /// Compiles a project dependency.
    ///
    fn compile(
        dependency: Self,
        path: &str,
        optimizer_settings: OptimizerSettings,
        is_system_mode: bool,
        include_metadata_hash: bool,
        debug_config: Option<DebugConfig>,
    ) -> anyhow::Result<String>;

    ///
    /// Resolves a full contract path.
    ///
    fn resolve_path(&self, identifier: &str) -> anyhow::Result<String>;

    ///
    /// Resolves a library address.
    ///
    fn resolve_library(&self, path: &str) -> anyhow::Result<String>;
}

///
/// The dummy dependency entity.
///
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

    ///
    /// Resolves a full contract path.
    ///
    fn resolve_path(&self, _identifier: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }

    ///
    /// Resolves a library address.
    ///
    fn resolve_library(&self, _path: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}
