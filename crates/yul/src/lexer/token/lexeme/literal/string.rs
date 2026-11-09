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

/// The largest code point encoded as a single byte.
const UTF8_ONE_BYTE_MAX: u16 = 0x7F;
/// The largest code point encoded as two bytes.
const UTF8_TWO_BYTE_MAX: u16 = 0x7FF;
/// The marker bits of the leading byte of a two byte sequence.
const UTF8_TWO_BYTE_LEAD: u16 = 0xC0;
/// The marker bits of the leading byte of a three byte sequence.
const UTF8_THREE_BYTE_LEAD: u16 = 0xE0;
/// The marker bits of a continuation byte.
const UTF8_CONTINUATION: u16 = 0x80;
/// The number of code point bits carried by a continuation byte.
const UTF8_CONTINUATION_BITS: u32 = 6;
/// Selects the code point bits carried by a continuation byte.
const UTF8_CONTINUATION_MASK: u16 = 0x3F;

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
    /// Fails if the decoded bytes are not valid UTF-8.
    pub fn unescape(&self) -> anyhow::Result<std::string::String> {
        std::string::String::from_utf8(self.unescape_bytes()?)
            .map_err(|error| anyhow::anyhow!("Invalid UTF-8 in string `{}`: {error}", self.inner))
    }

    /// Returns the bytes the string denotes, with its escape sequences decoded like solc's
    /// `Scanner::scanEscape`, which rejects any escape it does not know.
    pub fn unescape_bytes(&self) -> anyhow::Result<Vec<u8>> {
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
                    let code_point =
                        Self::hexadecimal_escape(&mut characters, CODEPOINT_ESCAPE_DIGITS)?;
                    bytes.extend(Self::encode_code_point(
                        u16::try_from(code_point).expect("four hexadecimal digits fit 16 bits"),
                    ));
                }
                Some('t') => bytes.push(b'\t'),
                Some('n') => bytes.push(b'\n'),
                Some('r') => bytes.push(b'\r'),
                Some(escaped @ ('\'' | '"' | '\\')) => {
                    bytes.push(u8::try_from(escaped).expect("quotes and backslash are ASCII"))
                }
                // Like solc, a backslash before a CR, LF or CRLF line break continues the
                // string on the next line.
                Some('\r') => {
                    characters.next_if_eq(&'\n');
                }
                Some('\n') => {}
                Some(other) => {
                    anyhow::bail!("Invalid escape `\\{other}` in string `{}`", self.inner)
                }
                None => anyhow::bail!("Unterminated escape in string `{}`", self.inner),
            }
        }

        Ok(bytes)
    }

    /// Encodes the code point of a `\u` escape as UTF-8 bytes.
    /// This mirrors solc's `Scanner::addUnicodeAsUTF8`, which also encodes surrogate code points
    /// (U+D800 to U+DFFF) that a Rust `char` cannot represent.
    /// The escape has four hex digits, so the encoding is at most three bytes long.
    fn encode_code_point(code_point: u16) -> Vec<u8> {
        let continuation = |shift: u32| {
            (UTF8_CONTINUATION | ((code_point >> shift) & UTF8_CONTINUATION_MASK)) as u8
        };

        if code_point <= UTF8_ONE_BYTE_MAX {
            vec![code_point as u8]
        } else if code_point <= UTF8_TWO_BYTE_MAX {
            vec![
                (UTF8_TWO_BYTE_LEAD | (code_point >> UTF8_CONTINUATION_BITS)) as u8,
                continuation(0),
            ]
        } else {
            vec![
                (UTF8_THREE_BYTE_LEAD | (code_point >> (2 * UTF8_CONTINUATION_BITS))) as u8,
                continuation(UTF8_CONTINUATION_BITS),
                continuation(0),
            ]
        }
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
            r"\x", r"\x0", r"\xg0", r"\x+1", r"\u00f", r"\u+0fc", r"\ud800", r"\xc3", r"\q", r"\",
            r"\u12é",
        ] {
            assert!(unescape(escaped).is_err(), "{escaped:?}");
        }
    }

    /// solc encodes a `\u` escape by its numeric range, surrogate code points included.
    #[test]
    fn unescape_bytes_encodes_code_points_like_solc() {
        for (escaped, bytes) in [
            (r"\u0041", vec![0x41]),
            (r"\u007f", vec![0x7f]),
            (r"\u0080", vec![0xc2, 0x80]),
            (r"\u07ff", vec![0xdf, 0xbf]),
            (r"\u0800", vec![0xe0, 0xa0, 0x80]),
            (r"\ud800", vec![0xed, 0xa0, 0x80]),
            (r"\udfff", vec![0xed, 0xbf, 0xbf]),
            (r"\uffff", vec![0xef, 0xbf, 0xbf]),
        ] {
            let unescaped = String::new(escaped.to_owned(), false).unescape_bytes();
            assert_eq!(unescaped.unwrap(), bytes, "{escaped:?}");
        }
    }
}
