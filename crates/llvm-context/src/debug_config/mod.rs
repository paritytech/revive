//! The debug configuration.

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use self::ir_type::IRType;

pub mod ir_type;

/// The debug configuration.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DebugConfig {
    /// The directory to dump the IRs to.
    pub output_directory: Option<PathBuf>,
    /// Whether debug info should be emitted.
    pub emit_debug_info: bool,
    /// The YUL debug output file path.
    ///
    /// Is expected to be configured when running in YUL mode.
    pub contract_path: Option<PathBuf>,
    /// The YUL input file path.
    ///
    /// Is expected to be configured when not running in YUL mode.
    pub yul_path: Option<PathBuf>,
}

impl DebugConfig {
    /// A shortcut constructor.
    pub const fn new(output_directory: Option<PathBuf>, emit_debug_info: bool) -> Self {
        Self {
            output_directory,
            emit_debug_info,
            contract_path: None,
            yul_path: None,
        }
    }

    /// Set the current YUL path.
    pub fn set_yul_path(&mut self, yul_path: &str) {
        self.yul_path = PathBuf::from(yul_path).into();
    }

    /// Set the current contract path.
    pub fn set_contract_path(&mut self, contract_path: &str) {
        self.contract_path = self.yul_source_path(contract_path);
    }

    /// Returns with the following precedence:
    /// 1. The YUL source path if it was configured.
    /// 2. The source YUL path from the debug output dir if it was configured.
    /// 3. `None` if there is no debug output directory.
    pub fn yul_source_path(&self, contract_path: &str) -> Option<PathBuf> {
        if let Some(path) = self.yul_path.as_ref() {
            return Some(path.clone());
        }

        self.output_directory.as_ref().map(|output_directory| {
            let mut file_path = output_directory.to_owned();
            let full_file_name = Self::full_file_name(contract_path, None, IRType::Yul);
            file_path.push(full_file_name);
            file_path
        })
    }

    /// Dumps the Yul IR.
    pub fn dump_yul(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        if let Some(file_path) = self.yul_source_path(contract_path) {
            std::fs::write(file_path, code)?;
        }

        Ok(())
    }

    /// Dumps the unoptimized LLVM IR.
    pub fn dump_llvm_ir_unoptimized(
        &self,
        contract_path: &str,
        module: &inkwell::module::Module,
    ) -> anyhow::Result<()> {
        if let Some(output_directory) = self.output_directory.as_ref() {
            let llvm_code = module.print_to_string().to_string();

            let mut file_path = output_directory.to_owned();
            let full_file_name =
                Self::full_file_name(contract_path, Some("unoptimized"), IRType::LLVM);
            file_path.push(full_file_name);
            std::fs::write(file_path, llvm_code)?;
        }

        Ok(())
    }

    /// Dumps the optimized LLVM IR.
    pub fn dump_llvm_ir_optimized(
        &self,
        contract_path: &str,
        module: &inkwell::module::Module,
    ) -> anyhow::Result<()> {
        if let Some(output_directory) = self.output_directory.as_ref() {
            let llvm_code = module.print_to_string().to_string();

            let mut file_path = output_directory.to_owned();
            let full_file_name =
                Self::full_file_name(contract_path, Some("optimized"), IRType::LLVM);
            file_path.push(full_file_name);
            std::fs::write(file_path, llvm_code)?;
        }

        Ok(())
    }

    /// Dumps the assembly.
    pub fn dump_assembly(&self, contract_path: &str, code: &str) -> anyhow::Result<()> {
        if let Some(output_directory) = self.output_directory.as_ref() {
            let mut file_path = output_directory.to_owned();
            let full_file_name = Self::full_file_name(contract_path, None, IRType::Assembly);
            file_path.push(full_file_name);
            std::fs::write(file_path, code)?;
        }

        Ok(())
    }

    /// Dumps the code object.
    pub fn dump_object(&self, contract_path: &str, code: &[u8]) -> anyhow::Result<()> {
        if let Some(output_directory) = self.output_directory.as_ref() {
            let mut file_path = output_directory.to_owned();
            let full_file_name = Self::full_file_name(contract_path, None, IRType::SO);
            file_path.push(full_file_name);
            std::fs::write(file_path, code)?;
        }

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
