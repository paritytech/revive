//!
//! The variable declaration statement.
//!

use std::collections::HashSet;

use inkwell::types::BasicType;
use inkwell::values::BasicValue;
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
use crate::yul::parser::statement::expression::function_call::name::Name as FunctionName;
use crate::yul::parser::statement::expression::Expression;

///
/// The Yul variable declaration statement.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VariableDeclaration {
    /// The location.
    pub location: Location,
    /// The variable bindings list.
    pub bindings: Vec<Identifier>,
    /// The variable initializing expression.
    pub expression: Option<Expression>,
}

impl VariableDeclaration {
    ///
    /// The element parser.
    ///
    pub fn parse(
        lexer: &mut Lexer,
        initial: Option<Token>,
    ) -> Result<(Self, Option<Token>), Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;
        let location = token.location;

        let (bindings, next) = Identifier::parse_typed_list(lexer, Some(token))?;
        for binding in bindings.iter() {
            match FunctionName::from(binding.inner.as_str()) {
                FunctionName::UserDefined(_) => continue,
                _function_name => {
                    return Err(ParserError::ReservedIdentifier {
                        location: binding.location,
                        identifier: binding.inner.to_owned(),
                    }
                    .into())
                }
            }
        }

        match crate::yul::parser::take_or_next(next, lexer)? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::Assignment),
                ..
            } => {}
            token => {
                return Ok((
                    Self {
                        location,
                        bindings,
                        expression: None,
                    },
                    Some(token),
                ))
            }
        }

        let expression = Expression::parse(lexer, None)?;

        Ok((
            Self {
                location,
                bindings,
                expression: Some(expression),
            },
            None,
        ))
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        self.expression
            .as_ref()
            .map_or_else(HashSet::new, |expression| {
                expression.get_missing_libraries()
            })
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for VariableDeclaration
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn into_llvm<'ctx>(
        mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> anyhow::Result<()> {
        if self.bindings.len() == 1 {
            let identifier = self.bindings.remove(0);
            let r#type = identifier.r#type.unwrap_or_default().into_llvm(context);
            let pointer = context.build_alloca(r#type, identifier.inner.as_str());
            context
                .current_function()
                .borrow_mut()
                .insert_stack_pointer(identifier.inner.clone(), pointer);

            let value = if let Some(expression) = self.expression {
                match expression.into_llvm(context)? {
                    Some(mut value) => {
                        if let Some(constant) = value.constant.take() {
                            context
                                .current_function()
                                .borrow_mut()
                                .yul_mut()
                                .insert_constant(identifier.inner, constant);
                        }

                        value.to_llvm()
                    }
                    None => r#type.const_zero().as_basic_value_enum(),
                }
            } else {
                r#type.const_zero().as_basic_value_enum()
            };
            context.build_store(pointer, value);
            return Ok(());
        }

        for (index, binding) in self.bindings.iter().enumerate() {
            let yul_type = binding
                .r#type
                .to_owned()
                .unwrap_or_default()
                .into_llvm(context);
            let pointer = context.build_alloca(
                yul_type.as_basic_type_enum(),
                format!("binding_{index}_pointer").as_str(),
            );
            context.build_store(pointer, yul_type.const_zero());
            context
                .current_function()
                .borrow_mut()
                .insert_stack_pointer(binding.inner.to_owned(), pointer);
        }

        let expression = match self.expression.take() {
            Some(expression) => expression,
            None => return Ok(()),
        };
        let location = expression.location();
        let expression = match expression.into_llvm(context)? {
            Some(expression) => expression,
            None => return Ok(()),
        };

        let llvm_type = context.structure_type(
            self.bindings
                .iter()
                .map(|binding| {
                    binding
                        .r#type
                        .to_owned()
                        .unwrap_or_default()
                        .into_llvm(context)
                        .as_basic_type_enum()
                })
                .collect::<Vec<inkwell::types::BasicTypeEnum<'ctx>>>()
                .as_slice(),
        );
        if expression.value.get_type() != llvm_type.as_basic_type_enum() {
            anyhow::bail!(
                "{} Assignment to {:?} received an invalid number of arguments",
                location,
                self.bindings
            );
        }
        let pointer = context.build_alloca(llvm_type, "bindings_pointer");
        context.build_store(pointer, expression.to_llvm());

        for (index, binding) in self.bindings.into_iter().enumerate() {
            let pointer = context.build_gep(
                pointer,
                &[
                    context.field_const(0),
                    context
                        .integer_type(era_compiler_common::BIT_LENGTH_X32)
                        .const_int(index as u64, false),
                ],
                binding.r#type.unwrap_or_default().into_llvm(context),
                format!("binding_{index}_gep_pointer").as_str(),
            );

            let value = context.build_load(pointer, format!("binding_{index}_value").as_str());
            let pointer = context
                .current_function()
                .borrow_mut()
                .get_stack_pointer(binding.inner.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{} Assignment to an undeclared variable `{}`",
                        binding.location,
                        binding.inner
                    )
                })?;
            context.build_store(pointer, value);
        }

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
                let basefee := 42
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
            Err(Error::ReservedIdentifier {
                location: Location::new(11, 21),
                identifier: "basefee".to_owned()
            }
            .into())
        );
    }
}
