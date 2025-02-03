//! The `solc --standard-json` output.

pub mod contract;
pub mod error;
pub mod source;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;
use sha3::Digest;

use crate::project::contract::ir::IR as ProjectContractIR;
use crate::project::contract::Contract as ProjectContract;
use crate::project::Project;
use crate::solc::version::Version as SolcVersion;
use crate::warning::Warning;
use crate::yul::lexer::Lexer;
use crate::yul::parser::statement::object::Object;

use self::contract::Contract;
use self::error::Error as SolcStandardJsonOutputError;
use self::source::Source;
/// The `solc --standard-json` output.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Output {
    /// The file-contract hashmap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contracts: Option<BTreeMap<String, BTreeMap<String, Contract>>>,
    /// The source code mapping data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<BTreeMap<String, Source>>,
    /// The compilation errors and warnings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<SolcStandardJsonOutputError>>,
    /// The `solc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The `solc` compiler long version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_version: Option<String>,
    /// The `resolc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revive_version: Option<String>,
}

impl Output {
    /// Converts the `solc` JSON output into a convenient project.
    pub fn try_to_project(
        &mut self,
        source_code_files: BTreeMap<String, String>,
        libraries: BTreeMap<String, BTreeMap<String, String>>,
        solc_version: &SolcVersion,
        debug_config: &revive_llvm_context::DebugConfig,
    ) -> anyhow::Result<Project> {
        let files = match self.contracts.as_ref() {
            Some(files) => files,
            None => match &self.errors {
                Some(errors) if errors.iter().any(|e| e.severity == "error") => {
                    anyhow::bail!(serde_json::to_string_pretty(errors).expect("Always valid"));
                }
                _ => &BTreeMap::new(),
            },
        };
        let mut project_contracts = BTreeMap::new();

        for (path, contracts) in files.iter() {
            for (name, contract) in contracts.iter() {
                let full_path = format!("{path}:{name}");

                let ir_optimized = match contract.ir_optimized.to_owned() {
                    Some(ir_optimized) => ir_optimized,
                    None => continue,
                };
                if ir_optimized.is_empty() {
                    continue;
                }

                debug_config.dump_yul(full_path.as_str(), ir_optimized.as_str())?;

                let mut lexer = Lexer::new(ir_optimized.to_owned());
                let object = Object::parse(&mut lexer, None).map_err(|error| {
                    anyhow::anyhow!("Contract `{}` parsing error: {:?}", full_path, error)
                })?;

                let source = ProjectContractIR::new_yul(ir_optimized.to_owned(), object);

                let source_code = source_code_files
                    .get(path.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Source code for path `{}` not found", path))?;
                let source_hash = sha3::Keccak256::digest(source_code.as_bytes()).into();

                let project_contract = ProjectContract::new(
                    full_path.clone(),
                    source_hash,
                    solc_version.to_owned(),
                    source,
                    contract.metadata.to_owned(),
                );
                project_contracts.insert(full_path, project_contract);
            }
        }

        Ok(Project::new(
            solc_version.to_owned(),
            project_contracts,
            libraries,
        ))
    }

    /// Traverses the AST and returns the list of additional errors and warnings.
    pub fn preprocess_ast(&mut self, suppressed_warnings: &[Warning]) -> anyhow::Result<()> {
        let sources = match self.sources.as_ref() {
            Some(sources) => sources,
            None => return Ok(()),
        };

        let mut messages = Vec::new();
        for (path, source) in sources.iter() {
            if let Some(ast) = source.ast.as_ref() {
                let mut polkavm_messages = Source::get_messages(ast, suppressed_warnings);
                for message in polkavm_messages.iter_mut() {
                    message.push_contract_path(path.as_str());
                }
                messages.extend(polkavm_messages);
            }
        }
        self.errors = match self.errors.take() {
            Some(mut errors) => {
                errors.extend(messages);
                Some(errors)
            }
            None => Some(messages),
        };

        Ok(())
    }
}
