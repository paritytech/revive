//! The YUL code block.

pub mod error;
pub mod identifier;
pub mod statement;
pub mod r#type;

use crate::lexer::error::Error as LexerError;
use crate::lexer::token::Token;
use crate::lexer::Lexer;

/// Returns the `token` value if it is `Some(_)`, otherwise takes the next token from the `stream`.
pub fn take_or_next(mut token: Option<Token>, lexer: &mut Lexer) -> Result<Token, LexerError> {
    match token.take() {
        Some(token) => Ok(token),
        None => lexer.next(),
    }
}
