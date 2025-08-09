//! The switch statement.

pub mod case;

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::lexeme::keyword::Keyword;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::parser::statement::block::Block;
use crate::parser::statement::expression::Expression;
use crate::visitor::AstNode;

use self::case::Case;

/// The Yul switch statement.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Switch {
    /// The location.
    pub location: Location,
    /// The expression being matched.
    pub expression: Expression,
    /// The non-default cases.
    pub cases: Vec<Case>,
    /// The optional default case, if `cases` do not cover all possible values.
    pub default: Option<Block>,
}

/// The parsing state.
pub enum State {
    /// After match expression.
    CaseOrDefaultKeyword,
    /// After `case`.
    CaseBlock,
    /// After `default`.
    DefaultBlock,
}

impl Switch {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let mut token = crate::parser::take_or_next(initial, lexer)?;
        let location = token.location;
        let mut state = State::CaseOrDefaultKeyword;

        let expression = Expression::parse(lexer, Some(token.clone()))?;
        let mut cases = Vec::new();
        let mut default = None;

        loop {
            match state {
                State::CaseOrDefaultKeyword => match lexer.peek()? {
                    _token @ Token {
                        lexeme: Lexeme::Keyword(Keyword::Case),
                        ..
                    } => {
                        token = _token;
                        state = State::CaseBlock;
                    }
                    _token @ Token {
                        lexeme: Lexeme::Keyword(Keyword::Default),
                        ..
                    } => {
                        token = _token;
                        state = State::DefaultBlock;
                    }
                    _token => {
                        token = _token;
                        break;
                    }
                },
                State::CaseBlock => {
                    lexer.next()?;
                    cases.push(Case::parse(lexer, None)?);
                    state = State::CaseOrDefaultKeyword;
                }
                State::DefaultBlock => {
                    lexer.next()?;
                    default = Some(Block::parse(lexer, None)?);
                    break;
                }
            }
        }

        if cases.is_empty() && default.is_none() {
            return Err(ParserError::InvalidToken {
                location: token.location,
                expected: vec!["case", "default"],
                found: token.lexeme.to_string(),
            }
            .into());
        }

        Ok(Self {
            location,
            expression,
            cases,
            default,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = HashSet::new();
        for case in self.cases.iter() {
            libraries.extend(case.get_missing_libraries());
        }
        if let Some(default) = &self.default {
            libraries.extend(default.get_missing_libraries());
        }
        libraries
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for Switch
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        context.set_debug_location(self.location.line, self.location.column, None)?;
        let scrutinee = self.expression.into_llvm(context)?;

        if self.cases.is_empty() {
            if let Some(block) = self.default {
                block.into_llvm(context)?;
            }
            return Ok(());
        }

        let current_block = context.basic_block();
        let join_block = context.append_basic_block("switch_join_block");

        let mut branches = Vec::with_capacity(self.cases.len());
        for (index, case) in self.cases.into_iter().enumerate() {
            let constant = case.literal.into_llvm(context)?.access(context)?;

            let expression_block = context
                .append_basic_block(format!("switch_case_branch_{}_block", index + 1).as_str());
            context.set_basic_block(expression_block);
            case.block.into_llvm(context)?;
            context.set_debug_location(self.location.line, self.location.column, None)?;
            context.build_unconditional_branch(join_block);

            branches.push((constant.into_int_value(), expression_block));
        }

        let default_block = match self.default {
            Some(default) => {
                let default_block = context.append_basic_block("switch_default_block");
                context.set_basic_block(default_block);
                default.into_llvm(context)?;
                context.build_unconditional_branch(join_block);
                default_block
            }
            None => join_block,
        };

        context.set_debug_location(self.location.line, self.location.column, None)?;
        context.set_basic_block(current_block);
        context.builder().build_switch(
            scrutinee
                .expect("Always exists")
                .access(context)?
                .into_int_value(),
            default_block,
            branches.as_slice(),
        )?;

        context.set_debug_location(self.location.line, self.location.column, None)?;
        context.set_basic_block(join_block);

        Ok(())
    }
}

impl AstNode for Switch {
    fn accept(&self, ast_visitor: &mut impl crate::visitor::AstVisitor) {
        ast_visitor.visit_switch(self);
    }

    fn visit_children(&self, ast_visitor: &mut impl crate::visitor::AstVisitor) {
        self.expression.accept(ast_visitor);

        for case in &self.cases {
            case.accept(ast_visitor);
        }

        if let Some(default) = self.default.as_ref() {
            default.accept(ast_visitor);
        }
    }

    fn location(&self) -> Location {
        self.location
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::token::location::Location;
    use crate::lexer::Lexer;
    use crate::parser::error::Error;
    use crate::parser::statement::object::Object;

    #[test]
    fn error_invalid_token_case() {
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
                switch 42
                    branch x {}
                    default {}
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
                location: Location::new(12, 21),
                expected: vec!["case", "default"],
                found: "branch".to_owned(),
            }
            .into())
        );
    }
}
