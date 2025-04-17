//! The assignment expression statement.

use std::collections::HashSet;

use inkwell::types::BasicType;
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
use crate::yul::parser::statement::expression::Expression;

/// The Yul assignment expression statement.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Assignment {
    /// The location.
    pub location: Location,
    /// The variable bindings.
    pub bindings: Vec<Identifier>,
    /// The initializing expression.
    pub initializer: Expression,
}

impl Assignment {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

        let (location, identifier) = match token {
            Token {
                location,
                lexeme: Lexeme::Identifier(identifier),
                ..
            } => (location, identifier),
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{identifier}"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };
        let length = identifier
            .inner
            .len()
            .try_into()
            .map_err(|_| Error::Parser(ParserError::InvalidLength))?;

        match lexer.peek()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::Assignment),
                ..
            } => {
                lexer.next()?;

                Ok(Self {
                    location,
                    bindings: vec![Identifier::new(location, identifier.inner)],
                    initializer: Expression::parse(lexer, None)?,
                })
            }
            Token {
                lexeme: Lexeme::Symbol(Symbol::Comma),
                ..
            } => {
                let (identifiers, next) = Identifier::parse_list(
                    lexer,
                    Some(Token::new(location, Lexeme::Identifier(identifier), length)),
                )?;

                match crate::yul::parser::take_or_next(next, lexer)? {
                    Token {
                        lexeme: Lexeme::Symbol(Symbol::Assignment),
                        ..
                    } => {}
                    token => {
                        return Err(ParserError::InvalidToken {
                            location: token.location,
                            expected: vec![":="],
                            found: token.lexeme.to_string(),
                        }
                        .into());
                    }
                }

                Ok(Self {
                    location,
                    bindings: identifiers,
                    initializer: Expression::parse(lexer, None)?,
                })
            }
            token => Err(ParserError::InvalidToken {
                location: token.location,
                expected: vec![":=", ","],
                found: token.lexeme.to_string(),
            }
            .into()),
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.initializer.get_missing_libraries()
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for Assignment
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(
        mut self,
        context: &mut revive_llvm_context::PolkaVMContext<D>,
    ) -> anyhow::Result<()> {
        context.set_debug_location(self.location.line, 0, None)?;

        let value = match self.initializer.into_llvm(context, None)? {
            Some(value) => value,
            None => return Ok(()),
        };

        if self.bindings.len() == 1 {
            let identifier = self.bindings.remove(0);
            let pointer = context
                .current_function()
                .borrow()
                .get_stack_pointer(identifier.inner.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{} Assignment to an undeclared variable `{}`",
                        identifier.location,
                        identifier.inner,
                    )
                })?;
            context.build_store(pointer, value.access(context)?)?;
            return Ok(());
        }
        let value = value.access(context)?;
        let llvm_type = value.into_struct_value().get_type();
        let tuple_pointer = context.build_alloca(llvm_type, "assignment_pointer");
        context.build_store(tuple_pointer, value)?;

        for (index, binding) in self.bindings.into_iter().enumerate() {
            context.set_debug_location(self.location.line, 0, None)?;

            let field_pointer = context.build_gep(
                tuple_pointer,
                &[
                    context.word_const(0),
                    context
                        .integer_type(revive_common::BIT_LENGTH_X32)
                        .const_int(index as u64, false),
                ],
                context.word_type().as_basic_type_enum(),
                format!("assignment_binding_{index}_gep_pointer").as_str(),
            );

            let binding_pointer = context
                .current_function()
                .borrow()
                .get_stack_pointer(binding.inner.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{} Assignment to an undeclared variable `{}`",
                        binding.location,
                        binding.inner,
                    )
                })?;
            let value = context.build_load(
                field_pointer,
                format!("assignment_binding_{index}_value").as_str(),
            )?;
            context.build_store(binding_pointer, value)?;
        }

        Ok(())
    }
}
