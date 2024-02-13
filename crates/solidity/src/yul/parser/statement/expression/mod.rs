//!
//! The expression statement.
//!

pub mod function_call;
pub mod literal;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::yul::error::Error;
use crate::yul::lexer::token::lexeme::symbol::Symbol;
use crate::yul::lexer::token::lexeme::Lexeme;
use crate::yul::lexer::token::location::Location;
use crate::yul::lexer::token::Token;
use crate::yul::lexer::Lexer;
use crate::yul::parser::error::Error as ParserError;
use crate::yul::parser::identifier::Identifier;

use self::function_call::FunctionCall;
use self::literal::Literal;

///
/// The Yul expression statement.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Expression {
    /// The function call subexpression.
    FunctionCall(FunctionCall),
    /// The identifier operand.
    Identifier(Identifier),
    /// The literal operand.
    Literal(Literal),
}

impl Expression {
    ///
    /// The element parser.
    ///
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

        let (location, identifier) = match token {
            Token {
                lexeme: Lexeme::Literal(_),
                ..
            } => return Ok(Self::Literal(Literal::parse(lexer, Some(token))?)),
            Token {
                location,
                lexeme: Lexeme::Identifier(identifier),
                ..
            } => (location, identifier),
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{literal}", "{identifier}"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };
        let length = identifier.inner.len();

        match lexer.peek()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::ParenthesisLeft),
                ..
            } => {
                lexer.next()?;
                Ok(Self::FunctionCall(FunctionCall::parse(
                    lexer,
                    Some(Token::new(location, Lexeme::Identifier(identifier), length)),
                )?))
            }
            _ => Ok(Self::Identifier(Identifier::new(
                location,
                identifier.inner,
            ))),
        }
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        match self {
            Self::FunctionCall(inner) => inner.get_missing_libraries(),
            Self::Identifier(_) => HashSet::new(),
            Self::Literal(_) => HashSet::new(),
        }
    }

    ///
    /// Returns the statement location.
    ///
    pub fn location(&self) -> Location {
        match self {
            Self::FunctionCall(inner) => inner.location,
            Self::Identifier(inner) => inner.location,
            Self::Literal(inner) => inner.location,
        }
    }

    ///
    /// Converts the expression into an LLVM value.
    ///
    pub fn into_llvm<'ctx, D>(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> anyhow::Result<Option<era_compiler_llvm_context::EraVMArgument<'ctx>>>
    where
        D: era_compiler_llvm_context::EraVMDependency + Clone,
    {
        match self {
            Self::Literal(literal) => literal
                .clone()
                .into_llvm(context)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "{} Invalid literal `{}`: {}",
                        literal.location,
                        literal.inner.to_string(),
                        error
                    )
                })
                .map(Some),
            Self::Identifier(identifier) => {
                let pointer = context
                    .current_function()
                    .borrow()
                    .get_stack_pointer(identifier.inner.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "{} Undeclared variable `{}`",
                            identifier.location,
                            identifier.inner,
                        )
                    })?;

                let constant = context
                    .current_function()
                    .borrow()
                    .yul()
                    .get_constant(identifier.inner.as_str());

                let value = context.build_load(pointer, identifier.inner.as_str());

                match constant {
                    Some(constant) => Ok(Some(
                        era_compiler_llvm_context::EraVMArgument::new_with_constant(
                            value, constant,
                        ),
                    )),
                    None => Ok(Some(value.into())),
                }
            }
            Self::FunctionCall(call) => Ok(call
                .into_llvm(context)?
                .map(era_compiler_llvm_context::EraVMArgument::new)),
        }
    }
}
