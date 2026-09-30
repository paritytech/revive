//! The string literal lexeme.

use serde::Deserialize;
use serde::Serialize;

use revive_common::BASE_HEXADECIMAL;

use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::lexeme::Literal;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;

/// The string literal lexeme.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct String {
    /// The inner string contents.
    pub inner: std::string::String,
    /// Whether the string is hexadecimal.
    pub is_hexadecimal: bool,
}

impl String {
    /// Creates a string literal value.
    pub fn new(inner: ::std::string::String, is_hexadecimal: bool) -> Self {
        Self {
            inner,
            is_hexadecimal,
        }
    }

    /// Returns the string with its escape sequences decoded.
    pub fn unescape(&self) -> anyhow::Result<std::string::String> {
        let mut bytes = Vec::with_capacity(self.inner.len());
        let mut characters = self.inner.chars();
        while let Some(character) = characters.next() {
            if character != '\\' {
                bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                continue;
            }

            match characters.next() {
                Some('x') => {
                    let digits: std::string::String = characters.by_ref().take(2).collect();
                    let byte = u8::from_str_radix(&digits, BASE_HEXADECIMAL).map_err(|error| {
                        anyhow::anyhow!("Invalid hexadecimal escape `\\x{digits}`: {error}")
                    })?;
                    bytes.push(byte);
                }
                Some('u') => {
                    let digits: std::string::String = characters.by_ref().take(4).collect();
                    let codepoint =
                        u32::from_str_radix(&digits, BASE_HEXADECIMAL).map_err(|error| {
                            anyhow::anyhow!("Invalid codepoint `{digits}`: {error}")
                        })?;
                    let unicode_character = char::from_u32(codepoint)
                        .ok_or_else(|| anyhow::anyhow!("Invalid codepoint {codepoint}"))?;
                    bytes.extend_from_slice(unicode_character.encode_utf8(&mut [0; 4]).as_bytes());
                }
                Some('t') => bytes.push(b'\t'),
                Some('n') => bytes.push(b'\n'),
                Some('r') => bytes.push(b'\r'),
                Some('\n') | None => {}
                Some(other) => bytes.extend_from_slice(other.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }

        std::string::String::from_utf8(bytes)
            .map_err(|error| anyhow::anyhow!("Invalid UTF-8 in string `{}`: {error}", self.inner))
    }

    /// Parses the value from the source code slice.
    pub fn parse(input: &str) -> Option<Token> {
        let mut length = 0;

        let is_string = input[length..].starts_with('"');
        let is_hex_string = input[length..].starts_with(r#"hex""#);

        if !is_string && !is_hex_string {
            return None;
        }

        if is_string {
            length += 1;
        }
        if is_hex_string {
            length += r#"hex""#.len();
        }

        let mut string = std::string::String::new();
        loop {
            if input[length..].starts_with('\\') {
                string.push(input.chars().nth(length).expect("Always exists"));
                string.push(input.chars().nth(length + 1).expect("Always exists"));
                length += 2;
                continue;
            }

            if input[length..].starts_with('"') {
                length += 1;
                break;
            }

            string.push(input.chars().nth(length).expect("Always exists"));
            length += 1;
        }

        let string = string
            .strip_prefix('"')
            .and_then(|string| string.strip_suffix('"'))
            .unwrap_or(string.as_str())
            .to_owned();

        let literal = Self::new(string, is_hex_string);
        let length = length
            .try_into()
            .expect("the YUL should be of reasonable size");

        Some(Token::new(
            Location::new(0, length),
            Lexeme::Literal(Literal::String(literal)),
            length,
        ))
    }
}

impl std::fmt::Display for String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}
