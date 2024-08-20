//! The debug configuration.

pub mod ir_type;

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use self::ir_type::IRType;

/// The debug configuration.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DebugConfig {
    /// The directory to dump the IRs to.
    pub output_directory: PathBuf,
}

impl DebugConfig {
    /// A shortcut constructor.
    pub fn new(output_directory: PathBuf) -> Self {
        Self { output_directory }
    }

    /// Dumps the Yul IR.
    pub fn dump_yul(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, None, IRType::Yul);
        file_path.push(full_file_name);
        std::fs::write(file_path, code)?;

        Ok(())
    }

    /// Dumps the EVM legacy assembly IR.
    pub fn dump_evmla(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, None, IRType::EVMLA);
        file_path.push(full_file_name);
        std::fs::write(file_path, code)?;

        Ok(())
    }

    /// Dumps the Ethereal IR.
    pub fn dump_ethir(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, None, IRType::EthIR);
        file_path.push(full_file_name);
        std::fs::write(file_path, code)?;

        Ok(())
    }

    /// Dumps the unoptimized LLVM IR.
    pub fn dump_llvm_ir_unoptimized(
        &self,
        contract_path: &str,
        module: &inkwell::module::Module,
    ) -> anyhow::Result<()> {
        let llvm_code = module.print_to_string().to_string();

        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, Some("unoptimized"), IRType::LLVM);
        file_path.push(full_file_name);
        std::fs::write(file_path, llvm_code)?;

        Ok(())
    }

    /// Dumps the optimized LLVM IR.
    pub fn dump_llvm_ir_optimized(
        &self,
        contract_path: &str,
        module: &inkwell::module::Module,
    ) -> anyhow::Result<()> {
        let llvm_code = module.print_to_string().to_string();

        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, Some("optimized"), IRType::LLVM);
        file_path.push(full_file_name);
        std::fs::write(file_path, llvm_code)?;

        Ok(())
    }

    /// Dumps the assembly.
    pub fn dump_assembly(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, None, IRType::Assembly);
        file_path.push(full_file_name);
        std::fs::write(file_path, code)?;

        Ok(())
    }

    /// Dumps the stage output as a json file suitable for use with --recursive-process
    #[cfg(debug_assertions)]
    pub fn dump_stage_output(
        &self,
        contract_path: &str,
        contract_suffix: Option<&str>,
        stage_json: &Vec<u8>,
    ) -> anyhow::Result<()> {
        let mut file_path = self.output_directory.to_owned();
        let full_file_name = Self::full_file_name(contract_path, contract_suffix, IRType::JSON);
        file_path.push(full_file_name);
        std::fs::write(file_path, stage_json)?;

        Ok(())
    }

    /// Creates a full file name, given the contract full path, suffix, and extension.
    fn full_file_name(contract_path: &str, suffix: Option<&str>, ir_type: IRType) -> String {
        let mut full_file_name = contract_path.replace('/', "_").replace(':', ".");
        if let Some(suffix) = suffix {
            full_file_name.push('.');
            full_file_name.push_str(suffix);
        }
        full_file_name.push('.');
        full_file_name.push_str(ir_type.file_extension());
        full_file_name
    }
}
