//! The string literal lexeme.

use serde::Deserialize;
use serde::Serialize;

use revive_common::BASE_HEXADECIMAL;

use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::lexeme::Literal;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;

/// The number of hexadecimal digits of a `\x` byte escape.
const BYTE_ESCAPE_DIGITS: usize = 2;
/// The number of hexadecimal digits of a `\u` codepoint escape.
const CODEPOINT_ESCAPE_DIGITS: usize = 4;

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
        let mut characters = self.inner.chars().peekable();
        while let Some(character) = characters.next() {
            if character != '\\' {
                bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                continue;
            }

            match characters.next() {
                Some('x') => {
                    let byte = Self::hexadecimal_escape(&mut characters, BYTE_ESCAPE_DIGITS)?;
                    bytes.push(u8::try_from(byte).expect("two hexadecimal digits fit a byte"));
                }
                Some('u') => {
                    let codepoint =
                        Self::hexadecimal_escape(&mut characters, CODEPOINT_ESCAPE_DIGITS)?;
                    let unicode_character = char::from_u32(codepoint)
                        .ok_or_else(|| anyhow::anyhow!("Invalid codepoint {codepoint}"))?;
                    bytes.extend_from_slice(unicode_character.encode_utf8(&mut [0; 4]).as_bytes());
                }
                Some('t') => bytes.push(b'\t'),
                Some('n') => bytes.push(b'\n'),
                Some('r') => bytes.push(b'\r'),
                // Like solc, a backslash before a CR, LF or CRLF line break continues the
                // string on the next line.
                Some('\r') => {
                    characters.next_if_eq(&'\n');
                }
                Some('\n') | None => {}
                Some(other) => bytes.extend_from_slice(other.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }

        std::string::String::from_utf8(bytes)
            .map_err(|error| anyhow::anyhow!("Invalid UTF-8 in string `{}`: {error}", self.inner))
    }

    /// Decodes the exactly `count` hexadecimal digits of a `\x` or `\u` escape.
    fn hexadecimal_escape(
        characters: &mut impl Iterator<Item = char>,
        count: usize,
    ) -> anyhow::Result<u32> {
        let digits: std::string::String = characters.take(count).collect();
        if digits.len() != count || !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            anyhow::bail!("Expected {count} hexadecimal digits in an escape, found `{digits}`");
        }
        Ok(u32::from_str_radix(&digits, BASE_HEXADECIMAL).expect("the digits are hexadecimal"))
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

#[cfg(test)]
mod tests {
    use crate::lexer::token::lexeme::literal::string::String;

    fn unescape(escaped: &str) -> anyhow::Result<std::string::String> {
        String::new(escaped.to_owned(), false).unescape()
    }

    /// The escapes solc emits when it prints a string literal.
    #[test]
    fn unescape_decodes_solc_escapes() {
        for (escaped, unescaped) in [
            ("a'b c.sol:L", "a'b c.sol:L"),
            (r"\xc3\xbc.sol:L", "ü.sol:L"),
            (r"a\x00b.sol:L", "a\0b.sol:L"),
            (r#"a\"b\\c.sol:L"#, r#"a"b\c.sol:L"#),
            (r"a\tb\nc\rd.sol:L", "a\tb\nc\rd.sol:L"),
        ] {
            assert_eq!(unescape(escaped).unwrap(), unescaped, "{escaped:?}");
        }
    }

    /// The escapes that only hand-written Yul contains.
    #[test]
    fn unescape_decodes_yul_only_escapes() {
        for (escaped, unescaped) in [
            (r"\u00fc.sol:L", "ü.sol:L"),
            (r"\'a.sol:L", "'a.sol:L"),
            ("a\\\nb.sol:L", "ab.sol:L"),
            ("a\\\rb.sol:L", "ab.sol:L"),
            ("a\\\r\nb.sol:L", "ab.sol:L"),
            ("a\\\r\n\nb.sol:L", "a\nb.sol:L"),
        ] {
            assert_eq!(unescape(escaped).unwrap(), unescaped, "{escaped:?}");
        }
    }

    #[test]
    fn unescape_rejects_malformed_escapes() {
        for escaped in [
            r"\x", r"\x0", r"\xg0", r"\x+1", r"\u00f", r"\u+0fc", r"\ud800", r"\xc3",
        ] {
            assert!(unescape(escaped).is_err(), "{escaped:?}");
        }
    }
}
