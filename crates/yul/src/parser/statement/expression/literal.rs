//! The YUL source code literal.

use inkwell::values::BasicValue;
use num::Num;
use num::One;
use num::Zero;
use serde::Deserialize;
use serde::Serialize;

use revive_common::BASE_DECIMAL;
use revive_common::BASE_HEXADECIMAL;
use revive_common::BYTE_LENGTH_WORD;
use revive_llvm_context::PolkaVMArgument;
use revive_llvm_context::PolkaVMContext;

use crate::error::Error;
use crate::lexer::token::lexeme::literal::boolean::Boolean as BooleanLiteral;
use crate::lexer::token::lexeme::literal::integer::Integer as IntegerLiteral;
use crate::lexer::token::lexeme::literal::Literal as LexicalLiteral;
use crate::lexer::token::lexeme::symbol::Symbol;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::parser::r#type::Type;
use crate::visitor::AstNode;
use crate::visitor::AstVisitor;

/// Represents a literal in YUL without differentiating its type.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Literal {
    /// The location.
    pub location: Location,
    /// The lexical literal.
    pub inner: LexicalLiteral,
    /// The type, if it has been explicitly specified.
    pub yul_type: Option<Type>,
}

impl Literal {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        let (location, literal) = match token {
            Token {
                lexeme: Lexeme::Literal(literal),
                location,
                ..
            } => (location, literal),
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{literal}"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let yul_type = match lexer.peek()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::Colon),
                ..
            } => {
                lexer.next()?;
                Some(Type::parse(lexer, None)?)
            }
            _ => None,
        };

        Ok(Self {
            location,
            inner: literal,
            yul_type,
        })
    }

    /// Converts the literal into its LLVM.
    pub fn into_llvm<'ctx>(
        self,
        context: &PolkaVMContext<'ctx>,
    ) -> anyhow::Result<PolkaVMArgument<'ctx>> {
        match self.inner {
            LexicalLiteral::Boolean(inner) => {
                let value = self
                    .yul_type
                    .unwrap_or_default()
                    .into_llvm(context)
                    .const_int(
                        match inner {
                            BooleanLiteral::False => 0,
                            BooleanLiteral::True => 1,
                        },
                        false,
                    )
                    .as_basic_value_enum();

                let constant = match inner {
                    BooleanLiteral::False => num::BigUint::zero(),
                    BooleanLiteral::True => num::BigUint::one(),
                };

                Ok(PolkaVMArgument::value(value).with_constant(constant))
            }
            LexicalLiteral::Integer(inner) => {
                let r#type = self.yul_type.unwrap_or_default().into_llvm(context);
                let value = match inner {
                    IntegerLiteral::Decimal { ref inner } => r#type.const_int_from_string(
                        inner.as_str(),
                        inkwell::types::StringRadix::Decimal,
                    ),
                    IntegerLiteral::Hexadecimal { ref inner } => r#type.const_int_from_string(
                        &inner["0x".len()..],
                        inkwell::types::StringRadix::Hexadecimal,
                    ),
                }
                .expect("The value is valid")
                .as_basic_value_enum();

                let constant = match inner {
                    IntegerLiteral::Decimal { ref inner } => {
                        num::BigUint::from_str_radix(inner.as_str(), BASE_DECIMAL)
                    }
                    IntegerLiteral::Hexadecimal { ref inner } => {
                        num::BigUint::from_str_radix(&inner["0x".len()..], BASE_HEXADECIMAL)
                    }
                }
                .expect("Always valid");

                Ok(PolkaVMArgument::value(value).with_constant(constant))
            }
            LexicalLiteral::String(inner) => {
                let r#type = self.yul_type.unwrap_or_default().into_llvm(context);
                let bytes = inner.to_bytes()?;
                let string = inner.inner;

                if bytes.len() > BYTE_LENGTH_WORD {
                    return Ok(
                        PolkaVMArgument::value(r#type.const_zero().as_basic_value_enum())
                            .with_original(string),
                    );
                }

                let mut word = [0; BYTE_LENGTH_WORD];
                word[..bytes.len()].copy_from_slice(&bytes);
                let hexadecimal: String = word.iter().map(|byte| format!("{byte:02x}")).collect();
                let value = r#type
                    .const_int_from_string(
                        hexadecimal.as_str(),
                        inkwell::types::StringRadix::Hexadecimal,
                    )
                    .expect("The value is valid")
                    .as_basic_value_enum();
                Ok(PolkaVMArgument::value(value).with_original(string))
            }
        }
    }
}

impl AstNode for Literal {
    fn accept(&self, ast_visitor: &mut impl AstVisitor) {
        ast_visitor.visit_literal(self);
    }

    fn visit_children(&self, _ast_visitor: &mut impl AstVisitor) {}

    fn location(&self) -> Location {
        self.location
    }
}
