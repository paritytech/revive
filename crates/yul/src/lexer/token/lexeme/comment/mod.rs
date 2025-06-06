//! The comment lexeme.

pub mod multi_line;
pub mod single_line;

use crate::lexer::token::Token;

use self::multi_line::Comment as MultiLineComment;
use self::single_line::Comment as SingleLineComment;

/// The comment lexeme.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Comment {
    /// The single-line comment.
    SingleLine(SingleLineComment),
    /// The multi-line comment.
    MultiLine(MultiLineComment),
}

impl Comment {
    /// Returns the comment's length, including the trimmed whitespace around it.
    pub fn parse(input: &str) -> Option<Token> {
        if input.starts_with(SingleLineComment::START) {
            Some(SingleLineComment::parse(input))
        } else if input.starts_with(MultiLineComment::START) {
            Some(MultiLineComment::parse(input))
        } else {
            None
        }
    }
}
