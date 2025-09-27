//! The block statement.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::lexeme::keyword::Keyword;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::visitor::AstNode;
use crate::visitor::AstVisitor;

use self::assignment::Assignment;
use self::block::Block;
use self::code::Code;
use self::expression::Expression;
use self::for_loop::ForLoop;
use self::function_definition::FunctionDefinition;
use self::if_conditional::IfConditional;
use self::object::Object;
use self::switch::Switch;
use self::variable_declaration::VariableDeclaration;

pub mod assignment;
pub mod block;
pub mod code;
pub mod expression;
pub mod for_loop;
pub mod function_definition;
pub mod if_conditional;
pub mod object;
pub mod switch;
pub mod variable_declaration;

/// The Yul block statement.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Statement {
    /// The object element.
    Object(Object),
    /// The code element.
    Code(Code),
    /// The code block.
    Block(Block),
    /// The expression.
    Expression(Expression),
    /// The `function` statement.
    FunctionDefinition(FunctionDefinition),
    /// The `let` statement.
    VariableDeclaration(VariableDeclaration),
    /// The `:=` existing variables reassignment statement.
    Assignment(Assignment),
    /// The `if` statement.
    IfConditional(IfConditional),
    /// The `switch` statement.
    Switch(Switch),
    /// The `for` statement.
    ForLoop(ForLoop),
    /// The `continue` statement.
    Continue(Location),
    /// The `break` statement.
    Break(Location),
    /// The `leave` statement.
    Leave(Location),
}

impl Statement {
    /// The element parser.
    pub fn parse(
        lexer: &mut Lexer,
        initial: Option<Token>,
    ) -> Result<(Self, Option<Token>), Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        match token {
            token @ Token {
                lexeme: Lexeme::Keyword(Keyword::Object),
                ..
            } => Ok((Statement::Object(Object::parse(lexer, Some(token))?), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Code),
                ..
            } => Ok((Statement::Code(Code::parse(lexer, None)?), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Function),
                ..
            } => Ok((
                Statement::FunctionDefinition(FunctionDefinition::parse(lexer, None)?),
                None,
            )),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Let),
                ..
            } => {
                let (statement, next) = VariableDeclaration::parse(lexer, None)?;
                Ok((Statement::VariableDeclaration(statement), next))
            }
            Token {
                lexeme: Lexeme::Keyword(Keyword::If),
                ..
            } => Ok((
                Statement::IfConditional(IfConditional::parse(lexer, None)?),
                None,
            )),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Switch),
                ..
            } => Ok((Statement::Switch(Switch::parse(lexer, None)?), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::For),
                ..
            } => Ok((Statement::ForLoop(ForLoop::parse(lexer, None)?), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Continue),
                location,
                ..
            } => Ok((Statement::Continue(location), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Break),
                location,
                ..
            } => Ok((Statement::Break(location), None)),
            Token {
                lexeme: Lexeme::Keyword(Keyword::Leave),
                location,
                ..
            } => Ok((Statement::Leave(location), None)),
            token => Err(ParserError::InvalidToken {
                location: token.location,
                expected: vec![
                    "object", "code", "function", "let", "if", "switch", "for", "continue",
                    "break", "leave",
                ],
                found: token.lexeme.to_string(),
            }
            .into()),
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        match self {
            Self::Object(inner) => inner.get_missing_libraries(),
            Self::Code(inner) => inner.get_missing_libraries(),
            Self::Block(inner) => inner.get_missing_libraries(),
            Self::Expression(inner) => inner.get_missing_libraries(),
            Self::FunctionDefinition(inner) => inner.get_missing_libraries(),
            Self::VariableDeclaration(inner) => inner.get_missing_libraries(),
            Self::Assignment(inner) => inner.get_missing_libraries(),
            Self::IfConditional(inner) => inner.get_missing_libraries(),
            Self::Switch(inner) => inner.get_missing_libraries(),
            Self::ForLoop(inner) => inner.get_missing_libraries(),
            Self::Continue(_) => BTreeSet::new(),
            Self::Break(_) => BTreeSet::new(),
            Self::Leave(_) => BTreeSet::new(),
        }
    }

    /// Returns the statement location.
    pub fn location(&self) -> Location {
        match self {
            Self::Object(inner) => inner.location,
            Self::Code(inner) => inner.location,
            Self::Block(inner) => inner.location,
            Self::Expression(inner) => inner.location(),
            Self::FunctionDefinition(inner) => inner.location,
            Self::VariableDeclaration(inner) => inner.location,
            Self::Assignment(inner) => inner.location,
            Self::IfConditional(inner) => inner.location,
            Self::Switch(inner) => inner.location,
            Self::ForLoop(inner) => inner.location,
            Self::Continue(location) => *location,
            Self::Break(location) => *location,
            Self::Leave(location) => *location,
        }
    }
}

impl AstNode for Statement {
    fn accept(&self, ast_visitor: &mut impl AstVisitor) {
        ast_visitor.visit_statement(self);
    }

    fn visit_children(&self, ast_visitor: &mut impl AstVisitor) {
        match self {
            Self::Object(inner) => inner.accept(ast_visitor),
            Self::Code(inner) => inner.accept(ast_visitor),
            Self::Block(inner) => inner.accept(ast_visitor),
            Self::Expression(inner) => inner.accept(ast_visitor),
            Self::FunctionDefinition(inner) => inner.accept(ast_visitor),
            Self::VariableDeclaration(inner) => inner.accept(ast_visitor),
            Self::Assignment(inner) => inner.accept(ast_visitor),
            Self::IfConditional(inner) => inner.accept(ast_visitor),
            Self::Switch(inner) => inner.accept(ast_visitor),
            Self::ForLoop(inner) => inner.accept(ast_visitor),
            Self::Continue(_location) => {}
            Self::Break(_location) => {}
            Self::Leave(_location) => {}
        }
    }

    fn location(&self) -> Location {
        self.location()
    }
}
