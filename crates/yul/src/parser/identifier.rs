//! The YUL source code identifier.

use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::lexeme::symbol::Symbol;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::r#type::Type;
use crate::visitor::AstNode;

/// The YUL source code identifier.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// The location.
    pub location: Location,
    /// The inner string.
    pub inner: String,
    /// The type, if it has been explicitly specified.
    pub r#type: Option<Type>,
}

impl Identifier {
    /// A shortcut constructor.
    pub fn new(location: Location, inner: String) -> Self {
        Self {
            location,
            inner,
            r#type: None,
        }
    }

    /// A shortcut constructor for a typed identifier.
    pub fn new_with_type(location: Location, inner: String, r#type: Option<Type>) -> Self {
        Self {
            location,
            inner,
            r#type,
        }
    }

    /// Parses the identifier list where the types cannot be specified.
    pub fn parse_list(
        lexer: &mut Lexer,
        mut initial: Option<Token>,
    ) -> Result<(Vec<Self>, Option<Token>), Error> {
        let mut result = Vec::new();

        let mut expected_comma = false;
        loop {
            let token = crate::parser::take_or_next(initial.take(), lexer)?;

            match token {
                Token {
                    location,
                    lexeme: Lexeme::Identifier(identifier),
                    ..
                } if !expected_comma => {
                    result.push(Self::new(location, identifier.inner));
                    expected_comma = true;
                }
                Token {
                    lexeme: Lexeme::Symbol(Symbol::Comma),
                    ..
                } if expected_comma => {
                    expected_comma = false;
                }
                token => return Ok((result, Some(token))),
            }
        }
    }

    /// Parses the identifier list where the types may be optionally specified.
    pub fn parse_typed_list(
        lexer: &mut Lexer,
        mut initial: Option<Token>,
    ) -> Result<(Vec<Self>, Option<Token>), Error> {
        let mut result = Vec::new();

        let mut expected_comma = false;
        loop {
            let token = crate::parser::take_or_next(initial.take(), lexer)?;

            match token {
                Token {
                    lexeme: Lexeme::Identifier(identifier),
                    location,
                    ..
                } if !expected_comma => {
                    let r#type = match lexer.peek()? {
                        Token {
                            lexeme: Lexeme::Symbol(Symbol::Colon),
                            ..
                        } => {
                            lexer.next()?;
                            Some(Type::parse(lexer, None)?)
                        }
                        _ => None,
                    };
                    result.push(Self::new_with_type(location, identifier.inner, r#type));
                    expected_comma = true;
                }
                Token {
                    lexeme: Lexeme::Symbol(Symbol::Comma),
                    ..
                } if expected_comma => {
                    expected_comma = false;
                }
                token => return Ok((result, Some(token))),
            }
        }
    }
}

impl AstNode for Identifier {
    fn accept(&self, ast_visitor: &mut impl crate::visitor::AstVisitor) {
        ast_visitor.visit_identifier(self);
    }

    fn visit_children(&self, _ast_visitor: &mut impl crate::visitor::AstVisitor) {}

    fn location(&self) -> Location {
        self.location
    }
}
