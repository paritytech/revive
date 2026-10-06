//! The debug configuration.

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use self::ir_type::IRType;

pub mod ir_type;

/// The escape character used when flattening a source path into a file name.
const ESCAPE: &str = "%";
/// The escaped form of [`ESCAPE`].
const ESCAPED_ESCAPE: &str = "%25";
/// The escaped form of an underscore, which otherwise stands for a path separator.
const ESCAPED_UNDERSCORE: &str = "%5F";
/// The escaped form of a colon inside the source path.
const ESCAPED_COLON: &str = "%3A";

/// The debug configuration.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DebugConfig {
    /// The directory to dump the IRs to.
    pub output_directory: Option<PathBuf>,
    /// Whether debug info should be emitted.
    pub emit_debug_info: bool,
    /// Whether to emit a machine-readable artifact for every newyork IR
    /// optimization stage, alongside the human-readable dumps.
    pub emit_ir_stages: bool,
    /// The YUL debug output file path.
    pub contract_path: Option<PathBuf>,
}

impl DebugConfig {
    /// A shortcut constructor.
    pub const fn new(
        output_directory: Option<PathBuf>,
        emit_debug_info: bool,
        emit_ir_stages: bool,
    ) -> Self {
        Self {
            output_directory,
            emit_debug_info,
            emit_ir_stages,
            contract_path: None,
        }
    }

    /// Set the current contract path.
    pub fn set_contract_path(&mut self, contract_path: &str) {
        self.contract_path = self.yul_source_path(contract_path);
    }

    pub fn yul_source_path(&self, contract_path: &str) -> Option<PathBuf> {
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
            let full_file_name = Self::full_file_name(contract_path, None, IRType::Object);
            file_path.push(full_file_name);
            std::fs::write(file_path, code)?;
        }

        Ok(())
    }

    /// Dumps a newyork IR debug artifact for the current contract.
    ///
    /// The file name is derived from the contract path (set via [`Self::set_contract_path`]) so it
    /// matches the other dumps and maps back to the source. `suffix` distinguishes artifacts
    /// (`"snapshot"`, `"heap"`, `"mem"`); `None` is the final optimized IR. A no-op when no output
    /// directory or contract path is configured.
    pub fn dump_newyork(&self, suffix: Option<&str>, text: &str) -> anyhow::Result<()> {
        if let Some(contract_path) = self.contract_path.as_ref() {
            let stem = contract_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let extension = IRType::NewYork.file_extension();
            let file_name = match suffix {
                Some(suffix) => format!("{stem}.{suffix}.{extension}"),
                None => format!("{stem}.{extension}"),
            };
            std::fs::write(contract_path.with_file_name(file_name), text)?;
        }

        Ok(())
    }

    /// Dumps one machine-readable newyork IR stage artifact.
    ///
    /// `stage_id` carries the pipeline ordinal, so the file names sort in pass
    /// order and no two stages of one contract can collide. Returns the file
    /// name for the stage manifest to reference, or `None` when stage export is
    /// off or no contract path is configured.
    pub fn dump_newyork_stage(&self, stage_id: &str, json: &str) -> anyhow::Result<Option<String>> {
        let Some(file_path) = self.newyork_stage_path(stage_id, IRType::NewYorkStage) else {
            return Ok(None);
        };
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the path was built from a file name")
            .to_owned();
        std::fs::write(file_path, json)?;

        Ok(Some(file_name))
    }

    /// Dumps the machine-readable Yul AST, the input the IR chain starts from.
    ///
    /// Returns the file name for the stage manifest to reference, or `None` when
    /// stage export is off or no contract path is configured.
    pub fn dump_yul_ast(&self, json: &str) -> anyhow::Result<Option<String>> {
        let Some(file_path) = self.newyork_stage_path(None, IRType::YulAst) else {
            return Ok(None);
        };
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the path was built from a file name")
            .to_owned();
        std::fs::write(file_path, json)?;

        Ok(Some(file_name))
    }

    /// Dumps the newyork IR stage manifest, which indexes the stage artifacts.
    ///
    /// A no-op when stage export is off or no contract path is configured.
    pub fn dump_newyork_manifest(&self, json: &str) -> anyhow::Result<()> {
        if let Some(file_path) = self.newyork_stage_path(None, IRType::NewYorkManifest) {
            std::fs::write(file_path, json)?;
        }

        Ok(())
    }

    /// Builds the path of a stage artifact, or `None` when stage export is off.
    ///
    /// The name is derived from the contract path the other dumps use, so every
    /// artifact of one contract shares a stem and maps back to its source.
    fn newyork_stage_path<'a>(
        &self,
        stage_id: impl Into<Option<&'a str>>,
        ir_type: IRType,
    ) -> Option<PathBuf> {
        if !self.emit_ir_stages {
            return None;
        }
        let contract_path = self.contract_path.as_ref()?;
        let stem = contract_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        let extension = ir_type.file_extension();
        let file_name = match stage_id.into() {
            Some(stage_id) => format!("{stem}.{stage_id}.{extension}"),
            None => format!("{stem}.{extension}"),
        };

        Some(contract_path.with_file_name(file_name))
    }

    /// Creates a full file name, given the contract full path, suffix, and extension.
    fn full_file_name(contract_path: &str, suffix: Option<&str>, ir_type: IRType) -> String {
        let (source_path, contract_name) = match contract_path.rsplit_once(':') {
            Some((source_path, contract_name)) => (source_path, Some(contract_name)),
            None => (contract_path, None),
        };
        let mut full_file_name = source_path
            .replace(ESCAPE, ESCAPED_ESCAPE)
            .replace('_', ESCAPED_UNDERSCORE)
            .replace(':', ESCAPED_COLON)
            .replace('/', "_");
        if let Some(contract_name) = contract_name {
            full_file_name.push('.');
            full_file_name.push_str(contract_name);
        }
        if let Some(suffix) = suffix {
            full_file_name.push('.');
            full_file_name.push_str(suffix);
        }
        full_file_name.push('.');
        full_file_name.push_str(ir_type.file_extension());
        full_file_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config pointing at `directory`, with the contract path set the way
    /// [`DebugConfig::set_contract_path`] sets it for a real compilation.
    fn config(directory: &std::path::Path, emit_ir_stages: bool) -> DebugConfig {
        let mut config = DebugConfig::new(Some(directory.to_path_buf()), false, emit_ir_stages);
        config.set_contract_path("src/Token.sol:Token");
        config
    }

    #[test]
    fn a_stage_is_named_after_the_contract_and_the_stage() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let config = config(directory.path(), true);

        let file_name = config
            .dump_newyork_stage("031.late0.inline", "{}")
            .expect("the stage should be written")
            .expect("stage export is on");

        assert_eq!(
            file_name,
            "src_Token.sol.Token.031.late0.inline.newyork.json"
        );
        let written = std::fs::read_to_string(directory.path().join(&file_name))
            .expect("the stage file should exist");
        assert_eq!(written, "{}");
    }

    #[test]
    fn the_manifest_sits_beside_the_stages() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let config = config(directory.path(), true);

        config
            .dump_newyork_manifest(r#"{"stages":[]}"#)
            .expect("the manifest should be written");

        let written =
            std::fs::read_to_string(directory.path().join("src_Token.sol.Token.stages.json"))
                .expect("the manifest should exist");
        assert_eq!(written, r#"{"stages":[]}"#);
    }

    #[test]
    fn the_yul_ast_sits_beside_the_stages() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let config = config(directory.path(), true);

        let file_name = config
            .dump_yul_ast(r#"{"identifier":"Token"}"#)
            .expect("the AST should be written")
            .expect("stage export is on");

        assert_eq!(file_name, "src_Token.sol.Token.yul.json");
        let written = std::fs::read_to_string(directory.path().join(&file_name))
            .expect("the AST file should exist");
        assert_eq!(written, r#"{"identifier":"Token"}"#);
    }

    #[test]
    fn nothing_is_written_when_stage_export_is_off() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let config = config(directory.path(), false);

        assert!(config
            .dump_newyork_stage("000.from_yul", "{}")
            .expect("the call should succeed")
            .is_none());
        assert!(config
            .dump_yul_ast("{}")
            .expect("the call should succeed")
            .is_none());
        config
            .dump_newyork_manifest("{}")
            .expect("the call should succeed");

        let entries = std::fs::read_dir(directory.path())
            .expect("the directory should be readable")
            .count();
        assert_eq!(entries, 0, "files were written with export off");
    }

    #[test]
    fn nothing_is_written_without_a_contract_path() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        // No `set_contract_path`, as when a dump happens before the contract is known.
        let config = DebugConfig::new(Some(directory.path().to_path_buf()), false, true);

        assert!(config
            .dump_newyork_stage("000.from_yul", "{}")
            .expect("the call should succeed")
            .is_none());
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("the directory should be readable")
                .count(),
            0,
        );
    }
}
