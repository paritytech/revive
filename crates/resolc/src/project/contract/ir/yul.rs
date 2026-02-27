//! The contract Yul source code.

use std::collections::BTreeSet;

use revive_yul::lexer::Lexer;
use serde::Deserialize;
use serde::Serialize;

use revive_yul::parser::statement::object::Object;

/// The contract Yul source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Yul {
    /// The Yul AST object.
    pub object: Object,
}

impl Yul {
    /// Transforms the `solc` standard JSON output contract into a Yul object.
    pub fn try_from_source(source_code: &str) -> anyhow::Result<Option<Self>> {
        if source_code.is_empty() {
            return Ok(None);
        };

        let mut lexer = Lexer::new(source_code.to_owned());
        let object = Object::parse(&mut lexer, None)
            .map_err(|error| anyhow::anyhow!("Yul parsing: {error:?}"))?;

        Ok(Some(Self { object }))
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
