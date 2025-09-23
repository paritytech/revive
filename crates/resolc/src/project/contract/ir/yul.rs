//! The contract Yul source code.

use std::collections::BTreeSet;

use revive_yul::lexer::Lexer;
use serde::Deserialize;
use serde::Serialize;

use revive_yul::parser::statement::object::Object;

/// he contract Yul source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Yul {
    /// The Yul AST object.
    pub object: Object,
}

impl Yul {
    /// Transforms the `solc` standard JSON output contract into a Yul object.
    pub fn try_from_source(
        path: &str,
        source_code: &str,
        debug_config: &revive_llvm_context::DebugConfig,
    ) -> anyhow::Result<Option<Self>> {
        if source_code.is_empty() {
            return Ok(None);
        };

        debug_config.dump_yul(path, source_code)?;

        let mut lexer = Lexer::new(source_code.to_owned());
        let object = Object::parse(&mut lexer, None)
            .map_err(|error| anyhow::anyhow!("Yul parsing: {error:?}"))?;

        Ok(Some(Self { object }))
    }

    /// Get the list of EVM dependencies.
    pub fn get_evm_dependencies(
        &self,
        runtime_code: Option<&Object>,
    ) -> revive_yul::dependencies::Dependencies {
        self.object.get_evm_dependencies(runtime_code)
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        self.object.get_missing_libraries()
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for Yul {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.object.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.object.into_llvm(context)
    }
}
