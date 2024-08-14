//! The source code block.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use inkwell::debug_info::AsDIScope;

use crate::yul::error::Error;
use crate::yul::lexer::token::lexeme::symbol::Symbol;
use crate::yul::lexer::token::lexeme::Lexeme;
use crate::yul::lexer::token::location::Location;
use crate::yul::lexer::token::Token;
use crate::yul::lexer::Lexer;
use crate::yul::parser::error::Error as ParserError;
use crate::yul::parser::statement::assignment::Assignment;
use crate::yul::parser::statement::expression::Expression;
use crate::yul::parser::statement::Statement;

/// The Yul source code block.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Block {
    /// The location.
    pub location: Location,
    /// The block statements.
    pub statements: Vec<Statement>,
}

impl Block {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

        let mut statements = Vec::new();

        let location = match token {
            Token {
                lexeme: Lexeme::Symbol(Symbol::BracketCurlyLeft),
                location,
                ..
            } => location,
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let mut remaining = None;

        loop {
            match crate::yul::parser::take_or_next(remaining.take(), lexer)? {
                token @ Token {
                    lexeme: Lexeme::Keyword(_),
                    ..
                } => {
                    let (statement, next) = Statement::parse(lexer, Some(token))?;
                    remaining = next;
                    statements.push(statement);
                }
                token @ Token {
                    lexeme: Lexeme::Literal(_),
                    ..
                } => {
                    statements
                        .push(Expression::parse(lexer, Some(token)).map(Statement::Expression)?);
                }
                token @ Token {
                    lexeme: Lexeme::Identifier(_),
                    ..
                } => match lexer.peek()? {
                    Token {
                        lexeme: Lexeme::Symbol(Symbol::Assignment),
                        ..
                    } => {
                        statements.push(
                            Assignment::parse(lexer, Some(token)).map(Statement::Assignment)?,
                        );
                    }
                    Token {
                        lexeme: Lexeme::Symbol(Symbol::Comma),
                        ..
                    } => {
                        statements.push(
                            Assignment::parse(lexer, Some(token)).map(Statement::Assignment)?,
                        );
                    }
                    _ => {
                        statements.push(
                            Expression::parse(lexer, Some(token)).map(Statement::Expression)?,
                        );
                    }
                },
                token @ Token {
                    lexeme: Lexeme::Symbol(Symbol::BracketCurlyLeft),
                    ..
                } => statements.push(Block::parse(lexer, Some(token)).map(Statement::Block)?),
                Token {
                    lexeme: Lexeme::Symbol(Symbol::BracketCurlyRight),
                    ..
                } => break,
                token => {
                    return Err(ParserError::InvalidToken {
                        location: token.location,
                        expected: vec!["{keyword}", "{expression}", "{identifier}", "{", "}"],
                        found: token.lexeme.to_string(),
                    }
                    .into());
                }
            }
        }

        Ok(Self {
            location,
            statements,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = HashSet::new();
        for statement in self.statements.iter() {
            libraries.extend(statement.get_missing_libraries());
        }
        libraries
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for Block
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        let current_function = context.current_function().borrow().name().to_owned();
        let current_block = context.basic_block();

        let mut functions = Vec::with_capacity(self.statements.len());
        let mut local_statements = Vec::with_capacity(self.statements.len());

        for statement in self.statements.into_iter() {
            match statement {
                Statement::FunctionDefinition(mut statement) => {
                    statement.declare(context)?;
                    functions.push(statement);
                }
                statement => local_statements.push(statement),
            }
        }

        for function in functions.into_iter() {
            function.into_llvm(context)?;
        }

        context.set_current_function(current_function.as_str(), Some(self.location.line))?;

        if let Some(dinfo) = context.debug_info() {
            let di_builder = dinfo.builder();
            let di_scope = dinfo.top_scope().expect("expected a debug-info scope");
            let di_block_scope = di_builder
                .create_lexical_block(
                    di_scope,
                    dinfo.compilation_unit().get_file(),
                    std::cmp::min(self.location.line, u32::MAX as usize) as u32,
                    0,
                )
                .as_debug_info_scope();
            context.push_debug_scope(di_block_scope);
            context.set_debug_location(self.location.line, 0, None)?;
        }

        context.set_basic_block(current_block);
        for statement in local_statements.into_iter() {
            context.set_debug_location(statement.location().line, 0, None)?;
            if context.basic_block().get_terminator().is_some() {
                break;
            }

            match statement {
                Statement::Block(block) => {
                    block.into_llvm(context)?;
                }
                Statement::Expression(expression) => {
                    expression.into_llvm(context)?;
                }
                Statement::VariableDeclaration(statement) => statement.into_llvm(context)?,
                Statement::Assignment(statement) => statement.into_llvm(context)?,
                Statement::IfConditional(statement) => statement.into_llvm(context)?,
                Statement::Switch(statement) => statement.into_llvm(context)?,
                Statement::ForLoop(statement) => statement.into_llvm(context)?,
                Statement::Continue(_location) => {
                    context.build_unconditional_branch(context.r#loop().continue_block);
                    break;
                }
                Statement::Break(_location) => {
                    context.build_unconditional_branch(context.r#loop().join_block);
                    break;
                }
                Statement::Leave(_location) => {
                    context.build_unconditional_branch(
                        context.current_function().borrow().return_block(),
                    );
                    break;
                }
                statement => anyhow::bail!(
                    "{} Unexpected local statement: {:?}",
                    statement.location(),
                    statement
                ),
            }
        }

        context.pop_debug_scope();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::yul::lexer::token::location::Location;
    use crate::yul::lexer::Lexer;
    use crate::yul::parser::error::Error;
    use crate::yul::parser::statement::object::Object;

    #[test]
    fn error_invalid_token_bracket_curly_left() {
        let input = r#"
object "Test" {
    code {
        {
            return(0, 0)
        }
    }
    object "Test_deployed" {
        code {
            {
                (
                    return(0, 0)
                }
            }
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(11, 17),
                expected: vec!["{keyword}", "{expression}", "{identifier}", "{", "}"],
                found: "(".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_statement() {
        let input = r#"
object "Test" {
    code {
        {
            return(0, 0)
        }
    }
    object "Test_deployed" {
        code {
            {
                :=
                return(0, 0)
            }
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(11, 17),
                expected: vec!["{keyword}", "{expression}", "{identifier}", "{", "}"],
                found: ":=".to_owned(),
            }
            .into())
        );
    }
}
