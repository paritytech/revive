//! The YUL source code type.

use serde::Deserialize;
use serde::Serialize;

use revive_common::BIT_LENGTH_BOOLEAN;
use revive_common::BIT_LENGTH_WORD;
use revive_llvm_context::PolkaVMContext;

use crate::error::Error;
use crate::lexer::token::lexeme::keyword::Keyword;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;

/// The YUL source code type.
/// The type is not currently in use, so all values have the `uint256` type by default.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Type {
    /// The `bool` type.
    Bool,
    /// The `int{N}` type.
    Int(usize),
    /// The `uint{N}` type.
    UInt(usize),
    /// The custom user-defined type.
    Custom(String),
}

impl Default for Type {
    fn default() -> Self {
        Self::UInt(BIT_LENGTH_WORD)
    }
}

impl Type {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        match token {
            Token {
                lexeme: Lexeme::Keyword(Keyword::Bool),
                ..
            } => Ok(Self::Bool),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Int(bitlength)),
                ..
            } => Ok(Self::Int(bitlength)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Uint(bitlength)),
                ..
            } => Ok(Self::UInt(bitlength)),
            Token {
                lexeme: Lexeme::Identifier(identifier),
                ..
            } => Ok(Self::Custom(identifier.inner)),
            token => Err(ParserError::InvalidToken {
                location: token.location,
                expected: vec!["{type}"],
                found: token.lexeme.to_string(),
            }
            .into()),
        }
    }

    /// Converts the type into its LLVM.
    pub fn into_llvm<'ctx>(self, context: &PolkaVMContext<'ctx>) -> inkwell::types::IntType<'ctx> {
        match self {
            Self::Bool => context.integer_type(BIT_LENGTH_BOOLEAN),
            Self::Int(bitlength) => context.integer_type(bitlength),
            Self::UInt(bitlength) => context.integer_type(bitlength),
            Self::Custom(_) => context.word_type(),
        }
    }
}
