//!
//! The literal lexeme.
//!

pub mod boolean;
pub mod integer;
pub mod string;

use serde::Deserialize;
use serde::Serialize;

use self::boolean::Boolean;
use self::integer::Integer;
use self::string::String;

///
/// The literal lexeme.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Literal {
    /// A boolean literal, like `true`, or `false`.
    Boolean(Boolean),
    /// An integer literal, like `42`, or `0xff`.
    Integer(Integer),
    /// A string literal, like `"message"`.
    String(String),
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Boolean(inner) => write!(f, "{inner}"),
            Self::Integer(inner) => write!(f, "{inner}"),
            Self::String(inner) => write!(f, "{inner}"),
        }
    }
}
