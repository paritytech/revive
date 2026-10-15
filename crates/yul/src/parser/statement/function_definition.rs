//! The function definition statement.

use std::collections::BTreeSet;

use inkwell::types::BasicType;

use serde::Deserialize;
use serde::Serialize;

use revive_common::BIT_LENGTH_X32;
use revive_llvm_context::PolkaVMContext;
use revive_llvm_context::PolkaVMFunctionReturn;
use revive_llvm_context::PolkaVMFunctionYulData;
use revive_llvm_context::PolkaVMWriteLLVM;

use crate::error::Error;
use crate::lexer::token::lexeme::symbol::Symbol;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::parser::identifier::Identifier;
use crate::parser::statement::block::Block;
use crate::parser::statement::expression::function_call::name::Name as FunctionName;
use crate::visitor::AstNode;
use crate::visitor::AstVisitor;

/// The function definition statement.
/// All functions are translated in two steps:
/// 1. The hoisted declaration
/// 2. The definition, which now has the access to all function signatures
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FunctionDefinition {
    /// The location.
    pub location: Location,
    /// The function identifier.
    pub identifier: String,
    /// The function formal arguments.
    pub arguments: Vec<Identifier>,
    /// The function return variables.
    pub result: Vec<Identifier>,
    /// The function body block.
    pub body: Block,
}

impl FunctionDefinition {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        let (location, identifier) = match token {
            Token {
                lexeme: Lexeme::Identifier(identifier),
                location,
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
        let identifier = Identifier::new(location, identifier.inner);

        match FunctionName::from(identifier.inner.as_str()) {
            FunctionName::UserDefined(_) => {}
            _function_name => {
                return Err(ParserError::ReservedIdentifier {
                    location,
                    identifier: identifier.inner,
                }
                .into())
            }
        }

        match lexer.next()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::ParenthesisLeft),
                ..
            } => {}
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["("],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        }

        let (arguments, next) = Identifier::parse_typed_list(lexer, None)?;
        match crate::parser::take_or_next(next, lexer)? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::ParenthesisRight),
                ..
            } => {}
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec![")"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        }

        let (result, next) = match lexer.peek()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::Arrow),
                ..
            } => {
                lexer.next()?;
                Identifier::parse_typed_list(lexer, None)?
            }
            Token {
                lexeme: Lexeme::Symbol(Symbol::BracketCurlyLeft),
                ..
            } => (vec![], None),
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["->", "{"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let body = Block::parse(lexer, next)?;

        Ok(Self {
            location,
            identifier: identifier.inner,
            arguments,
            result,
            body,
        })
    }

    /// Gets the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        self.body.get_missing_libraries()
    }
}

impl PolkaVMWriteLLVM for FunctionDefinition {
    fn declare(&mut self, context: &mut PolkaVMContext) -> anyhow::Result<()> {
        context.set_debug_location(self.location.line, self.location.column, None)?;
        let argument_types: Vec<_> = self
            .arguments
            .iter()
            .map(|argument| {
                let yul_type = argument.r#type.to_owned().unwrap_or_default();
                yul_type.into_llvm(context).as_basic_type_enum()
            })
            .collect();

        let function_type = context.function_type(argument_types, self.result.len());

        let function = context.add_function(
            self.identifier.as_str(),
            function_type,
            self.result.len(),
            Some(inkwell::module::Linkage::Internal),
            Some((self.location.line, self.location.column)),
            true,
        )?;
        function
            .borrow_mut()
            .set_yul_data(PolkaVMFunctionYulData::default());

        Ok(())
    }

    fn into_llvm(mut self, context: &mut PolkaVMContext) -> anyhow::Result<()> {
        context.set_current_function(
            self.identifier.as_str(),
            Some((self.location.line, self.location.column)),
            true,
        )?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        let r#return = context.current_function().borrow().r#return();
        match r#return {
            PolkaVMFunctionReturn::None => {}
            PolkaVMFunctionReturn::Primitive { pointer } => {
                let identifier = self.result.pop().expect("Always exists");

                let r#type = identifier.r#type.unwrap_or_default();
                context.build_store(pointer, r#type.into_llvm(context).const_zero())?;
                context
                    .current_function()
                    .borrow_mut()
                    .insert_stack_pointer(identifier.inner, pointer);
            }
            PolkaVMFunctionReturn::Compound { pointer, .. } => {
                for (index, identifier) in self.result.into_iter().enumerate() {
                    let r#type = identifier.r#type.unwrap_or_default().into_llvm(context);
                    let pointer = context.build_gep(
                        pointer,
                        &[
                            context.word_const(0),
                            context
                                .integer_type(BIT_LENGTH_X32)
                                .const_int(index as u64, false),
                        ],
                        context.word_type(),
                        format!("return_{index}_gep_pointer").as_str(),
                    );
                    context.build_store(pointer, r#type.const_zero())?;
                    context
                        .current_function()
                        .borrow_mut()
                        .insert_stack_pointer(identifier.inner.clone(), pointer);
                }
            }
        };

        let argument_types: Vec<_> = self
            .arguments
            .iter()
            .map(|argument| {
                let yul_type = argument.r#type.to_owned().unwrap_or_default();
                yul_type.into_llvm(context)
            })
            .collect();
        for (index, argument) in self.arguments.iter().enumerate() {
            let pointer = context.build_alloca(argument_types[index], argument.inner.as_str());
            context
                .current_function()
                .borrow_mut()
                .insert_stack_pointer(argument.inner.clone(), pointer);
            context.build_store(
                pointer,
                context.current_function().borrow().get_nth_param(index),
            )?;
        }

        self.body.into_llvm(context)?;
        context.set_debug_location(self.location.line, self.location.column, None)?;

        match context
            .basic_block()
            .get_last_instruction()
            .map(|instruction| instruction.get_opcode())
        {
            Some(inkwell::values::InstructionOpcode::Br) => {}
            Some(inkwell::values::InstructionOpcode::Switch) => {}
            _ => context
                .build_unconditional_branch(context.current_function().borrow().return_block()),
        }

        context.set_basic_block(context.current_function().borrow().return_block());
        match context.current_function().borrow().r#return() {
            PolkaVMFunctionReturn::None => {
                context.build_return(None);
            }
            PolkaVMFunctionReturn::Primitive { pointer } => {
                let return_value = context.build_load(pointer, "return_value")?;
                context.build_return(Some(&return_value));
            }
            PolkaVMFunctionReturn::Compound { pointer, .. } => {
                let return_value = context.build_load(pointer, "return_value")?;
                context.build_return(Some(&return_value));
            }
        }

        context.pop_debug_scope();

        Ok(())
    }
}

impl AstNode for FunctionDefinition {
    fn accept(&self, ast_visitor: &mut impl AstVisitor) {
        ast_visitor.visit_function_definition(self);
    }

    fn visit_children(&self, ast_visitor: &mut impl AstVisitor) {
        for argument in &self.arguments {
            argument.accept(ast_visitor);
        }

        self.body.accept(ast_visitor);

        for result in &self.result {
            result.accept(ast_visitor);
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
    fn error_invalid_token_identifier() {
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
                return(0, 0)
            }

            function 256() -> result {
                result := 42
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
                location: Location::new(14, 22),
                expected: vec!["{identifier}"],
                found: "256".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_parenthesis_left() {
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
                return(0, 0)
            }

            function test{) -> result {
                result := 42
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
                location: Location::new(14, 26),
                expected: vec!["("],
                found: "{".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_parenthesis_right() {
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
                return(0, 0)
            }

            function test(} -> result {
                result := 42
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
                location: Location::new(14, 27),
                expected: vec![")"],
                found: "}".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_arrow_or_bracket_curly_left() {
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
                return(0, 0)
            }

            function test() := result {
                result := 42
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
                location: Location::new(14, 29),
                expected: vec!["->", "{"],
                found: ":=".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_reserved_identifier() {
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
                return(0, 0)
            }

            function basefee() -> result {
                result := 42
            }
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::ReservedIdentifier {
                location: Location::new(14, 22),
                identifier: "basefee".to_owned()
            }
            .into())
        );
    }
}
