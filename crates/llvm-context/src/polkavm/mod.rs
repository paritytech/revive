//! The LLVM context library.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::debug_config::DebugConfig;
use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::{PolkaVMTarget, PolkaVMTargetMachine};

use anyhow::Context as AnyhowContext;
use polkavm_common::program::ProgramBlob;
use polkavm_disassembler::{Disassembler, DisassemblyFormat};
use revive_common::{
    Keccak256, ObjectFormat, BIT_LENGTH_ETH_ADDRESS, BIT_LENGTH_WORD, BYTE_LENGTH_ETH_ADDRESS,
    BYTE_LENGTH_WORD,
};
use revive_linker::elf::ElfLinker;
use revive_linker::pvm::polkavm_linker;

use self::context::build::Build;
use self::context::Context;
pub use self::r#const::*;

pub mod r#const;
pub mod context;
pub mod evm;

/// Get a [Build] from contract bytecode and its auxilliary data.
pub fn build(
    bytecode: &[u8],
    metadata_hash: Option<[u8; BYTE_LENGTH_WORD]>,
) -> anyhow::Result<Build> {
    Ok(Build::new(metadata_hash, bytecode.to_owned()))
}

/// Disassembles the PolkaVM blob into assembly text representation.
pub fn disassemble(
    contract_path: &str,
    bytecode: &[u8],
    debug_config: &DebugConfig,
) -> anyhow::Result<String> {
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
        .with_context(|| format!("Failed to disassemble contract: {contract_path}"))?;

    let assembly_text = String::from_utf8(disassembled_code).with_context(|| {
        format!("Failed to convert disassembled code to string for contract: {contract_path}")
    })?;

    debug_config.dump_assembly(contract_path, &assembly_text)?;

    Ok(assembly_text)
}

/// Computes the PVM bytecode hash.
pub fn hash(bytecode_buffer: &[u8]) -> [u8; BYTE_LENGTH_WORD] {
    Keccak256::from_slice(bytecode_buffer)
        .as_bytes()
        .try_into()
        .expect("the bytecode hash should be word sized")
}

/// Links the `bytecode` with `linker_symbols` and `factory_dependencies`.
pub fn link(
    bytecode: &[u8],
    linker_symbols: &BTreeMap<String, [u8; BYTE_LENGTH_ETH_ADDRESS]>,
    factory_dependencies: &BTreeMap<String, [u8; BYTE_LENGTH_WORD]>,
    strip_binary: bool,
) -> anyhow::Result<(Vec<u8>, ObjectFormat)> {
    Ok(match ObjectFormat::try_from(bytecode) {
        Ok(format @ ObjectFormat::PVM) => (bytecode.to_vec(), format),
        Ok(ObjectFormat::ELF) => {
            let symbol_names = symbol_names(bytecode)?;
            let factory_dependencies = factory_dependencies
                .iter()
                .filter(|(name, _)| symbol_names.contains(name.as_str()))
                .map(|(name, hash)| (name.to_owned(), *hash))
                .collect();
            let symbols = build_symbols(linker_symbols, &factory_dependencies)?;
            let bytecode_linked = ElfLinker::setup()?.link(bytecode, &symbols)?;
            match polkavm_linker(&bytecode_linked, strip_binary) {
                Ok(pvm) => (pvm, ObjectFormat::PVM),
                Err(error)
                    if error
                        .to_string()
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty())
                        .all(|line| line.contains("found undefined symbol")) =>
                {
                    (bytecode.to_vec(), ObjectFormat::ELF)
                }
                Err(error) => return Err(error),
            }
        }
        Err(error) => panic!("ICE: linker: {error}"),
    })
}

/// Returns the linker symbol name of the library address at `path`.
pub fn library_address_symbol(path: &str) -> String {
    format!("{GLOBAL_LIBRARY_ADDRESS_PREFIX}{path}")
}

/// The returned module defines given `linker_symbols` and `factory_dependencies` global values.
pub fn build_symbols(
    linker_symbols: &BTreeMap<String, [u8; BYTE_LENGTH_ETH_ADDRESS]>,
    factory_dependencies: &BTreeMap<String, [u8; BYTE_LENGTH_WORD]>,
) -> anyhow::Result<Vec<u8>> {
    let context = inkwell::context::Context::create();
    let module = context.create_module("symbols");
    let word_type = context
        .custom_width_int_type(NonZeroU32::new(BIT_LENGTH_WORD as u32).expect("const is non-zero"))
        .expect("valid integer width");
    let address_type = context
        .custom_width_int_type(
            NonZeroU32::new(BIT_LENGTH_ETH_ADDRESS as u32).expect("const is non-zero"),
        )
        .expect("valid integer width");

    for (name, value) in linker_symbols {
        let global_value = module.add_global(
            address_type,
            Default::default(),
            &library_address_symbol(name),
        );
        global_value.set_linkage(inkwell::module::Linkage::External);
        global_value.set_initializer(
            &address_type
                .const_int_from_string(
                    hex::encode(value).as_str(),
                    inkwell::types::StringRadix::Hexadecimal,
                )
                .expect("should be valid"),
        );
    }

    for (name, value) in factory_dependencies {
        let global_value = module.add_global(word_type, Default::default(), name);
        global_value.set_linkage(inkwell::module::Linkage::External);
        global_value.set_initializer(
            &word_type
                .const_int_from_string(
                    hex::encode(value).as_str(),
                    inkwell::types::StringRadix::Hexadecimal,
                )
                .expect("should be valid"),
        );
    }

    let target_machine =
        PolkaVMTargetMachine::new(PolkaVMTarget::PVM, &OptimizerSettings::none(), false)?;
    let buffer = target_machine
        .write_to_memory_buffer(&module)
        .expect("ICE: the symbols module should be valid");
    Ok(buffer.as_slice().to_vec())
}
/// Implemented by items which are translated into LLVM IR.
pub trait WriteLLVM {
    /// Declares the entity in the LLVM IR.
    /// Is usually performed in order to use the item before defining it.
    fn declare(&mut self, _context: &mut Context) -> anyhow::Result<()> {
        Ok(())
    }

    /// Translates the entity into LLVM IR.
    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()>;
}

/// The dummy LLVM writable entity.
#[derive(Debug, Default, Clone)]
pub struct DummyLLVMWritable {}

impl WriteLLVM for DummyLLVMWritable {
    fn into_llvm(self, _context: &mut Context) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Returns the names of all symbols in the ELF object `bytecode`.
fn symbol_names(bytecode: &[u8]) -> anyhow::Result<BTreeSet<String>> {
    let mut buffer = bytecode.to_vec();
    buffer.push(0);
    let memory_buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range_copy(&buffer, "object");
    let binary_file = memory_buffer
        .create_binary_file(None)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut symbols = binary_file
        .get_symbols()
        .context("the ELF object should have a symbol table")?;

    let mut names = BTreeSet::new();
    while let Some(symbol) = symbols.next_symbol() {
        if let Some(name) = symbol.get_name() {
            names.insert(name.to_string_lossy().into_owned());
        }
    }
    Ok(names)
}
