//! The YUL code.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use revive_llvm_context::PolkaVMContext;
use revive_llvm_context::PolkaVMWriteLLVM;

use crate::error::Error;
use crate::lexer::token::lexeme::keyword::Keyword;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::parser::statement::block::Block;
use crate::visitor::AstNode;
use crate::visitor::AstVisitor;

/// The YUL code entity, which is the first block of the object.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Code {
    /// The location.
    pub location: Location,
    /// The main block.
    pub block: Block,
}

impl Code {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        let location = match token {
            Token {
                lexeme: Lexeme::Keyword(Keyword::Code),
                location,
                ..
            } => location,
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["code"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let block = Block::parse(lexer, None)?;

        Ok(Self { location, block })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        self.block.get_missing_libraries()
    }
}

impl PolkaVMWriteLLVM for Code {
    fn into_llvm(self, context: &mut PolkaVMContext) -> anyhow::Result<()> {
        self.block.into_llvm(context)?;

        // The EVM lets the code return implicitly.
        revive_llvm_context::polkavm_evm_return::stop(context)?;

        Ok(())
    }
}

impl AstNode for Code {
    fn accept(&self, ast_visitor: &mut impl AstVisitor) {
        ast_visitor.visit_code(self);
    }

    fn visit_children(&self, ast_visitor: &mut impl AstVisitor) {
        self.block.accept(ast_visitor);
    }

    fn location(&self) -> Location {
        self.location
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::token::location::Location;
    use crate::lexer::Lexer;
    use crate::parser::error::Error;
    use crate::parser::statement::object::Object;

    #[test]
    fn error_invalid_token_code() {
        let input = r#"
object "Test" {
    data {
        {
            return(0, 0)
        }
    }
    object "Test_deployed" {
        code {
            {
                return(0, 0)
            }
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(3, 5),
                expected: vec!["code"],
                found: "data".to_owned(),
            }
            .into())
        );
    }
}
