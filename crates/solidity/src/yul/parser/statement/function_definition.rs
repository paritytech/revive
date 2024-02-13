//!
//! The function definition statement.
//!

use std::collections::BTreeSet;
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
use crate::yul::parser::statement::block::Block;
use crate::yul::parser::statement::expression::function_call::name::Name as FunctionName;

///
/// The function definition statement.
///
/// All functions are translated in two steps:
/// 1. The hoisted declaration
/// 2. The definition, which now has the access to all function signatures
///
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
    /// The function LLVM attributes encoded in the identifier.
    pub attributes: BTreeSet<era_compiler_llvm_context::EraVMAttribute>,
}

impl FunctionDefinition {
    /// The LLVM attribute section prefix.
    pub const LLVM_ATTRIBUTE_PREFIX: &'static str = "$llvm_";

    /// The LLVM attribute section suffix.
    pub const LLVM_ATTRIBUTE_SUFFIX: &'static str = "_llvm$";

    ///
    /// The element parser.
    ///
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

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

        let (mut arguments, next) = Identifier::parse_typed_list(lexer, None)?;
        if identifier
            .inner
            .contains(era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_PREFIX)
        {
            if arguments.is_empty() {
                return Err(ParserError::InvalidNumberOfArguments {
                    location,
                    identifier: identifier.inner,
                    expected: 1,
                    found: arguments.len(),
                }
                .into());
            }

            arguments.remove(0);
        }
        if identifier.inner.contains(
            era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_EXCEPTION_HANDLER,
        ) && !arguments.is_empty()
        {
            return Err(ParserError::InvalidNumberOfArguments {
                location,
                identifier: identifier.inner,
                expected: 0,
                found: arguments.len(),
            }
            .into());
        }

        match crate::yul::parser::take_or_next(next, lexer)? {
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

        let attributes = Self::get_llvm_attributes(&identifier)?;

        Ok(Self {
            location,
            identifier: identifier.inner,
            arguments,
            result,
            body,
            attributes,
        })
    }

    ///
    /// Gets the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.body.get_missing_libraries()
    }

    ///
    /// Gets the list of LLVM attributes provided in the function name.
    ///
    pub fn get_llvm_attributes(
        identifier: &Identifier,
    ) -> Result<BTreeSet<era_compiler_llvm_context::EraVMAttribute>, Error> {
        let mut valid_attributes = BTreeSet::new();

        let llvm_begin = identifier.inner.find(Self::LLVM_ATTRIBUTE_PREFIX);
        let llvm_end = identifier.inner.find(Self::LLVM_ATTRIBUTE_SUFFIX);
        let attribute_string = if let (Some(llvm_begin), Some(llvm_end)) = (llvm_begin, llvm_end) {
            if llvm_begin < llvm_end {
                &identifier.inner[llvm_begin + Self::LLVM_ATTRIBUTE_PREFIX.len()..llvm_end]
            } else {
                return Ok(valid_attributes);
            }
        } else {
            return Ok(valid_attributes);
        };

        let mut invalid_attributes = BTreeSet::new();
        for value in attribute_string.split('_') {
            match era_compiler_llvm_context::EraVMAttribute::try_from(value) {
                Ok(attribute) => valid_attributes.insert(attribute),
                Err(value) => invalid_attributes.insert(value),
            };
        }

        if !invalid_attributes.is_empty() {
            return Err(ParserError::InvalidAttributes {
                location: identifier.location,
                values: invalid_attributes,
            }
            .into());
        }

        Ok(valid_attributes)
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for FunctionDefinition
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        let argument_types: Vec<_> = self
            .arguments
            .iter()
            .map(|argument| {
                let yul_type = argument.r#type.to_owned().unwrap_or_default();
                yul_type.into_llvm(context).as_basic_type_enum()
            })
            .collect();

        let function_type = context.function_type(
            argument_types,
            self.result.len(),
            self.identifier
                .starts_with(era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_PREFIX),
        );

        let function = context.add_function(
            self.identifier.as_str(),
            function_type,
            self.result.len(),
            Some(inkwell::module::Linkage::Private),
        )?;
        era_compiler_llvm_context::EraVMFunction::set_attributes(
            context.llvm(),
            function.borrow().declaration(),
            self.attributes.clone().into_iter().collect(),
            true,
        );
        function
            .borrow_mut()
            .set_yul_data(era_compiler_llvm_context::EraVMFunctionYulData::default());

        Ok(())
    }

    fn into_llvm(
        mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        context.set_current_function(self.identifier.as_str())?;
        let r#return = context.current_function().borrow().r#return();

        context.set_basic_block(context.current_function().borrow().entry_block());
        match r#return {
            era_compiler_llvm_context::EraVMFunctionReturn::None => {}
            era_compiler_llvm_context::EraVMFunctionReturn::Primitive { pointer } => {
                let identifier = self.result.pop().expect("Always exists");
                let r#type = identifier.r#type.unwrap_or_default();
                context.build_store(pointer, r#type.into_llvm(context).const_zero());
                context
                    .current_function()
                    .borrow_mut()
                    .insert_stack_pointer(identifier.inner, pointer);
            }
            era_compiler_llvm_context::EraVMFunctionReturn::Compound { pointer, .. } => {
                for (index, identifier) in self.result.into_iter().enumerate() {
                    let r#type = identifier.r#type.unwrap_or_default().into_llvm(context);
                    let pointer = context.build_gep(
                        pointer,
                        &[
                            context.field_const(0),
                            context
                                .integer_type(era_compiler_common::BIT_LENGTH_X32)
                                .const_int(index as u64, false),
                        ],
                        context.field_type(),
                        format!("return_{index}_gep_pointer").as_str(),
                    );
                    context.build_store(pointer, r#type.const_zero());
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
        for (mut index, argument) in self.arguments.iter().enumerate() {
            let pointer = context.build_alloca(argument_types[index], argument.inner.as_str());
            context
                .current_function()
                .borrow_mut()
                .insert_stack_pointer(argument.inner.clone(), pointer);
            if self
                .identifier
                .starts_with(era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_PREFIX)
                && matches!(
                    context.current_function().borrow().r#return(),
                    era_compiler_llvm_context::EraVMFunctionReturn::Compound { .. }
                )
                && context.is_system_mode()
            {
                index += 1;
            }
            context.build_store(
                pointer,
                context.current_function().borrow().get_nth_param(index),
            );
        }

        self.body.into_llvm(context)?;
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
            era_compiler_llvm_context::EraVMFunctionReturn::None => {
                context.build_return(None);
            }
            era_compiler_llvm_context::EraVMFunctionReturn::Primitive { pointer } => {
                let return_value = context.build_load(pointer, "return_value");
                context.build_return(Some(&return_value));
            }
            era_compiler_llvm_context::EraVMFunctionReturn::Compound { pointer, .. }
                if context.current_function().borrow().name().starts_with(
                    era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_PREFIX,
                ) =>
            {
                context.build_return(Some(&pointer.value));
            }
            era_compiler_llvm_context::EraVMFunctionReturn::Compound { pointer, .. } => {
                let return_value = context.build_load(pointer, "return_value");
                context.build_return(Some(&return_value));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::yul::lexer::token::location::Location;
    use crate::yul::lexer::Lexer;
    use crate::yul::parser::error::Error;
    use crate::yul::parser::statement::object::Object;

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
    fn error_invalid_number_of_arguments_near_call_abi() {
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

            function ZKSYNC_NEAR_CALL_test() -> result {
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
            Err(Error::InvalidNumberOfArguments {
                location: Location::new(14, 22),
                identifier: "ZKSYNC_NEAR_CALL_test".to_owned(),
                expected: 1,
                found: 0,
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_number_of_arguments_near_call_abi_catch() {
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

            function ZKSYNC_CATCH_NEAR_CALL(length) {
                revert(0, length)
            }
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidNumberOfArguments {
                location: Location::new(14, 22),
                identifier: "ZKSYNC_CATCH_NEAR_CALL".to_owned(),
                expected: 0,
                found: 1,
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

    #[test]
    fn error_invalid_attributes_single() {
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

            function test_$llvm_UnknownAttribute_llvm$_test() -> result {
                result := 42
            }
        }
    }
}
    "#;
        let mut invalid_attributes = BTreeSet::new();
        invalid_attributes.insert("UnknownAttribute".to_owned());

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidAttributes {
                location: Location::new(14, 22),
                values: invalid_attributes,
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_attributes_multiple_repeated() {
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

            function test_$llvm_UnknownAttribute1_UnknownAttribute1_UnknownAttribute2_llvm$_test() -> result {
                result := 42
            }
        }
    }
}
    "#;
        let mut invalid_attributes = BTreeSet::new();
        invalid_attributes.insert("UnknownAttribute1".to_owned());
        invalid_attributes.insert("UnknownAttribute2".to_owned());

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidAttributes {
                location: Location::new(14, 22),
                values: invalid_attributes,
            }
            .into())
        );
    }
}
