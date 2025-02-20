//! The YUL object.

use std::collections::HashSet;

use inkwell::debug_info::AsDIScope;

use serde::Deserialize;
use serde::Serialize;

use crate::yul::error::Error;
use crate::yul::lexer::token::lexeme::keyword::Keyword;
use crate::yul::lexer::token::lexeme::literal::Literal;
use crate::yul::lexer::token::lexeme::symbol::Symbol;
use crate::yul::lexer::token::lexeme::Lexeme;
use crate::yul::lexer::token::location::Location;
use crate::yul::lexer::token::Token;
use crate::yul::lexer::Lexer;
use crate::yul::parser::error::Error as ParserError;
use crate::yul::parser::statement::code::Code;

/// The upper-level YUL object, representing the deploy code.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Object {
    /// The location.
    pub location: Location,
    /// The identifier.
    pub identifier: String,
    /// The code.
    pub code: Code,
    /// The optional inner object, representing the runtime code.
    pub inner_object: Option<Box<Self>>,
    /// The factory dependency objects, which are represented by nested Yul object. The nested
    /// objects are duplicates of the upper-level objects describing the dependencies, so only
    /// their identifiers are preserved. The identifiers are used to address upper-level objects.
    pub factory_dependencies: HashSet<String>,
}

impl Object {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

        let location = match token {
            Token {
                lexeme: Lexeme::Keyword(Keyword::Object),
                location,
                ..
            } => location,
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["object"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let identifier = match lexer.next()? {
            Token {
                lexeme: Lexeme::Literal(Literal::String(literal)),
                ..
            } => literal.inner,
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{string}"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };
        let is_runtime_code = identifier.ends_with("_deployed");

        match lexer.next()? {
            Token {
                lexeme: Lexeme::Symbol(Symbol::BracketCurlyLeft),
                ..
            } => {}
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        }

        let code = Code::parse(lexer, None)?;
        let mut inner_object = None;
        let mut factory_dependencies = HashSet::new();

        if !is_runtime_code {
            inner_object = match lexer.peek()? {
                Token {
                    lexeme: Lexeme::Keyword(Keyword::Object),
                    ..
                } => {
                    let mut object = Self::parse(lexer, None)?;

                    if format!("{identifier}_deployed") != object.identifier {
                        return Err(ParserError::InvalidObjectName {
                            location: object.location,
                            expected: format!("{identifier}_deployed"),
                            found: object.identifier,
                        }
                        .into());
                    }

                    factory_dependencies.extend(object.factory_dependencies.drain());
                    Some(Box::new(object))
                }
                _ => None,
            };

            if let Token {
                lexeme: Lexeme::Identifier(identifier),
                ..
            } = lexer.peek()?
            {
                if identifier.inner.as_str() == "data" {
                    let _data = lexer.next()?;
                    let _identifier = lexer.next()?;
                    let _metadata = lexer.next()?;
                }
            };
        }

        loop {
            match lexer.next()? {
                Token {
                    lexeme: Lexeme::Symbol(Symbol::BracketCurlyRight),
                    ..
                } => break,
                token @ Token {
                    lexeme: Lexeme::Keyword(Keyword::Object),
                    ..
                } => {
                    let dependency = Self::parse(lexer, Some(token))?;
                    factory_dependencies.insert(dependency.identifier);
                }
                Token {
                    lexeme: Lexeme::Identifier(identifier),
                    ..
                } if identifier.inner.as_str() == "data" => {
                    let _identifier = lexer.next()?;
                    let _metadata = lexer.next()?;
                }
                token => {
                    return Err(ParserError::InvalidToken {
                        location: token.location,
                        expected: vec!["object", "}"],
                        found: token.lexeme.to_string(),
                    }
                    .into());
                }
            }
        }

        Ok(Self {
            location,
            identifier,
            code,
            inner_object,
            factory_dependencies,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut missing_libraries = self.code.get_missing_libraries();
        if let Some(inner_object) = &self.inner_object {
            missing_libraries.extend(inner_object.get_missing_libraries());
        }
        missing_libraries
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for Object
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn declare(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<D>,
    ) -> anyhow::Result<()> {
        revive_llvm_context::PolkaVMImmutableDataLoadFunction.declare(context)?;
        revive_llvm_context::PolkaVMLoadHeapWordFunction.declare(context)?;
        revive_llvm_context::PolkaVMStoreHeapWordFunction.declare(context)?;

        let mut entry = revive_llvm_context::PolkaVMEntryFunction::default();
        entry.declare(context)?;

        revive_llvm_context::PolkaVMDeployCodeFunction::new(
            revive_llvm_context::PolkaVMDummyLLVMWritable::default(),
        )
        .declare(context)?;
        revive_llvm_context::PolkaVMRuntimeCodeFunction::new(
            revive_llvm_context::PolkaVMDummyLLVMWritable::default(),
        )
        .declare(context)?;

        for name in [
            revive_llvm_context::PolkaVMFunctionDeployCode,
            revive_llvm_context::PolkaVMFunctionRuntimeCode,
            revive_llvm_context::PolkaVMFunctionEntry,
        ]
        .into_iter()
        {
            context
                .get_function(name)
                .expect("Always exists")
                .borrow_mut()
                .set_yul_data(revive_llvm_context::PolkaVMFunctionYulData::default());
        }

        entry.into_llvm(context)?;

        revive_llvm_context::PolkaVMImmutableDataLoadFunction.into_llvm(context)?;
        revive_llvm_context::PolkaVMLoadHeapWordFunction.into_llvm(context)?;
        revive_llvm_context::PolkaVMStoreHeapWordFunction.into_llvm(context)?;

        Ok(())
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        if let Some(debug_info) = context.debug_info() {
            let di_builder = debug_info.builder();
            let object_name: &str = self.identifier.as_str();
            let di_parent_scope = debug_info
                .top_scope()
                .expect("expected an existing debug-info scope");
            let object_scope = di_builder.create_namespace(di_parent_scope, object_name, true);
            context.push_debug_scope(object_scope.as_debug_info_scope());
        }

        if self.identifier.ends_with("_deployed") {
            revive_llvm_context::PolkaVMRuntimeCodeFunction::new(self.code).into_llvm(context)?;
        } else {
            revive_llvm_context::PolkaVMDeployCodeFunction::new(self.code).into_llvm(context)?;
        }
        context.set_debug_location(self.location.line, 0, None)?;

        if let Some(object) = self.inner_object {
            object.into_llvm(context)?;
        }
        context.set_debug_location(self.location.line, 0, None)?;

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
    fn error_invalid_token_object() {
        let input = r#"
class "Test" {
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
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(2, 1),
                expected: vec!["object"],
                found: "class".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_identifier() {
        let input = r#"
object 256 {
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
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(2, 8),
                expected: vec!["{string}"],
                found: "256".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_bracket_curly_left() {
        let input = r#"
object "Test" (
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
        }
    }
}
    "#;

        let mut lexer = Lexer::new(input.to_owned());
        let result = Object::parse(&mut lexer, None);
        assert_eq!(
            result,
            Err(Error::InvalidToken {
                location: Location::new(2, 15),
                expected: vec!["{"],
                found: "(".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_token_object_inner() {
        let input = r#"
object "Test" {
    code {
        {
            return(0, 0)
        }
    }
    class "Test_deployed" {
        code {
            {
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
                location: Location::new(8, 5),
                expected: vec!["object", "}"],
                found: "class".to_owned(),
            }
            .into())
        );
    }

    #[test]
    fn error_invalid_object_name() {
        let input = r#"
object "Test" {
    code {
        {
            return(0, 0)
        }
    }
    object "Invalid" {
        code {
            {
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
            Err(Error::InvalidObjectName {
                location: Location::new(8, 5),
                expected: "Test_deployed".to_owned(),
                found: "Invalid".to_owned(),
            }
            .into())
        );
    }
}
