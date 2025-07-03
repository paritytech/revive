//! The function call subexpression.

pub mod name;
pub mod verbatim;

use std::collections::HashSet;

use inkwell::values::BasicValue;
use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::lexeme::literal::Literal as LexicalLiteral;
use crate::lexer::token::lexeme::symbol::Symbol;
use crate::lexer::token::lexeme::Lexeme;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::error::Error as ParserError;
use crate::parser::statement::expression::literal::Literal;
use crate::parser::statement::expression::Expression;

use self::name::Name;

/// The Yul function call subexpression.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    /// The location.
    pub location: Location,
    /// The function name.
    pub name: Name,
    /// The function arguments expression list.
    pub arguments: Vec<Expression>,
}

impl FunctionCall {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;

        let (location, name) = match token {
            Token {
                lexeme: Lexeme::Identifier(identifier),
                location,
                ..
            } => (location, Name::from(identifier.inner.as_str())),
            token => {
                return Err(ParserError::InvalidToken {
                    location: token.location,
                    expected: vec!["{identifier}"],
                    found: token.lexeme.to_string(),
                }
                .into());
            }
        };

        let mut arguments = Vec::new();
        loop {
            let argument = match lexer.next()? {
                Token {
                    lexeme: Lexeme::Symbol(Symbol::ParenthesisRight),
                    ..
                } => break,
                token => Expression::parse(lexer, Some(token))?,
            };

            arguments.push(argument);

            match lexer.peek()? {
                Token {
                    lexeme: Lexeme::Symbol(Symbol::Comma),
                    ..
                } => {
                    lexer.next()?;
                    continue;
                }
                Token {
                    lexeme: Lexeme::Symbol(Symbol::ParenthesisRight),
                    ..
                } => {
                    lexer.next()?;
                    break;
                }
                _ => break,
            }
        }

        Ok(Self {
            location,
            name,
            arguments,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = HashSet::new();

        if let Name::LinkerSymbol = self.name {
            let _argument = self.arguments.first().expect("Always exists");
            if let Expression::Literal(Literal {
                inner: LexicalLiteral::String(library_path),
                ..
            }) = self.arguments.first().expect("Always exists")
            {
                libraries.insert(library_path.to_string());
            }
            return libraries;
        }

        for argument in self.arguments.iter() {
            libraries.extend(argument.get_missing_libraries());
        }
        libraries
    }

    /// Converts the function call into an LLVM value.
    pub fn into_llvm<'ctx, D>(
        mut self,
        bindings: &[(String, revive_llvm_context::PolkaVMPointer<'ctx>)],
        context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    ) -> anyhow::Result<()>
    where
        D: revive_llvm_context::PolkaVMDependency + Clone,
    {
        let location = self.location;

        match self.name {
            Name::UserDefined(name) => {
                let mut values = Vec::with_capacity(bindings.len() + self.arguments.len());
                for (n, argument) in self.arguments.into_iter().rev().enumerate() {
                    let id = format!("arg_{n}");
                    let binding_pointer = context.build_alloca(context.word_type(), &id);
                    let value = argument
                        .into_llvm(&[(id, binding_pointer)], context)?
                        .expect("Always exists")
                        .as_pointer(context)?
                        .value
                        .as_basic_value_enum();
                    values.push(value);
                }
                values.reverse();

                let values = bindings
                    .into_iter()
                    .map(|(_, pointer)| pointer.value.as_basic_value_enum())
                    .chain(values.into_iter())
                    .collect::<Vec<_>>();

                let function = context.get_function(name.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("{} Undeclared function `{}`", location, name)
                })?;

                let expected_arguments_count =
                    function.borrow().declaration().value.count_params() as usize;
                if expected_arguments_count != values.len() {
                    anyhow::bail!(
                        "{} Function `{}` expected {} arguments, found {}",
                        location,
                        name,
                        expected_arguments_count,
                        values.len()
                    );
                }

                let _return_value = context.build_call(
                    function.borrow().declaration(),
                    values.as_slice(),
                    format!("{name}_call").as_str(),
                );

                Ok(())
            }

            Name::Add => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::addition(
                    context,
                    bindings,
                    arguments[0].into_pointer_value(),
                    arguments[1].into_pointer_value(),
                )?;
                Ok(())
            }
            Name::Sub => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::subtraction(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Mul => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::multiplication(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Div => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::division(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Mod => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::remainder(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Sdiv => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::division_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Smod => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_arithmetic::remainder_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::Lt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::ULT,
                )
                .map(Some)
            }
            Name::Gt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::UGT,
                )
                .map(Some)
            }
            Name::Eq => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            Name::IsZero => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    context.word_const(0),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            Name::Slt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SLT,
                )
                .map(Some)
            }
            Name::Sgt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SGT,
                )
                .map(Some)
            }

            Name::Or => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::or(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Xor => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Not => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    context.word_type().const_all_ones(),
                )
                .map(Some)
            }
            Name::And => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::and(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Shl => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::shift_left(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Shr => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::shift_right(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Sar => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::shift_right_arithmetic(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Byte => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_bitwise::byte(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Pop => {
                let _arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                Ok(None)
            }

            Name::AddMod => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_math::add_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            Name::MulMod => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_math::mul_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            Name::Exp => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_math::exponent(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::SignExtend => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_math::sign_extend(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::Keccak256 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_crypto::sha3(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::MLoad => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_memory::load(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            Name::MStore => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_memory::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::MStore8 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_memory::store_byte(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::MCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                let destination = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    arguments[0].into_int_value(),
                    "mcopy_destination",
                );
                let source = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    arguments[1].into_int_value(),
                    "mcopy_source",
                );

                context.build_memcpy(
                    destination,
                    source,
                    arguments[2].into_int_value(),
                    "mcopy_size",
                )?;
                Ok(None)
            }

            Name::SLoad => {
                let arguments = self.pop_arguments::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_storage::load(context, &arguments[0]).map(Some)
            }
            Name::SStore => {
                let arguments = self.pop_arguments::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_storage::store(
                    context,
                    &arguments[0],
                    &arguments[1],
                )
                .map(|_| None)
            }
            Name::TLoad => {
                let arguments = self.pop_arguments::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_storage::transient_load(context, &arguments[0])
                    .map(Some)
            }
            Name::TStore => {
                let arguments = self.pop_arguments::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_storage::transient_store(
                    context,
                    &arguments[0],
                    &arguments[1],
                )
                .map(|_| None)
            }
            Name::LoadImmutable => {
                let mut arguments = self.pop_arguments::<D, 1>(context)?;
                let key = arguments[0].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `load_immutable` literal is missing", location)
                })?;
                let offset = context
                    .solidity_mut()
                    .get_or_allocate_immutable(key.as_str())
                    / revive_common::BYTE_LENGTH_WORD;
                let index = context.xlen_type().const_int(offset as u64, false);
                revive_llvm_context::polkavm_evm_immutable::load(context, index).map(Some)
            }
            Name::SetImmutable => {
                let mut arguments = self.pop_arguments::<D, 3>(context)?;
                let key = arguments[1].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `load_immutable` literal is missing", location)
                })?;
                let offset = context.solidity_mut().allocate_immutable(key.as_str())
                    / revive_common::BYTE_LENGTH_WORD;
                let index = context.xlen_type().const_int(offset as u64, false);
                let value = arguments[2].access(context)?.into_int_value();
                revive_llvm_context::polkavm_evm_immutable::store(context, index, value)
                    .map(|_| None)
            }
            Name::CallDataLoad => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;

                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    revive_llvm_context::PolkaVMCodeType::Deploy => {
                        Ok(Some(context.word_const(0).as_basic_value_enum()))
                    }
                    revive_llvm_context::PolkaVMCodeType::Runtime => {
                        revive_llvm_context::polkavm_evm_calldata::load(
                            context,
                            arguments[0].into_int_value(),
                        )
                        .map(Some)
                    }
                }
            }
            Name::CallDataSize => {
                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    revive_llvm_context::PolkaVMCodeType::Deploy => {
                        Ok(Some(context.word_const(0).as_basic_value_enum()))
                    }
                    revive_llvm_context::PolkaVMCodeType::Runtime => {
                        revive_llvm_context::polkavm_evm_calldata::size(context).map(Some)
                    }
                }
            }
            Name::CallDataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;

                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    revive_llvm_context::PolkaVMCodeType::Deploy => {
                        let calldata_size =
                            revive_llvm_context::polkavm_evm_calldata::size(context)?;

                        revive_llvm_context::polkavm_evm_calldata::copy(
                            context,
                            arguments[0].into_int_value(),
                            calldata_size.into_int_value(),
                            arguments[2].into_int_value(),
                        )
                        .map(|_| None)
                    }
                    revive_llvm_context::PolkaVMCodeType::Runtime => {
                        revive_llvm_context::polkavm_evm_calldata::copy(
                            context,
                            arguments[0].into_int_value(),
                            arguments[1].into_int_value(),
                            arguments[2].into_int_value(),
                        )
                        .map(|_| None)
                    }
                }
            }
            Name::CodeSize => {
                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    revive_llvm_context::PolkaVMCodeType::Deploy => {
                        revive_llvm_context::polkavm_evm_calldata::size(context).map(Some)
                    }
                    revive_llvm_context::PolkaVMCodeType::Runtime => {
                        revive_llvm_context::polkavm_evm_ext_code::size(context, None).map(Some)
                    }
                }
            }
            Name::CodeCopy => {
                if let revive_llvm_context::PolkaVMCodeType::Runtime = context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    anyhow::bail!(
                        "{} The `CODECOPY` instruction is not supported in the runtime code",
                        location,
                    );
                }

                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_calldata::copy(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(|_| None)
            }
            Name::ReturnDataSize => {
                revive_llvm_context::polkavm_evm_return_data::size(context).map(Some)
            }
            Name::ReturnDataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_return_data::copy(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(|_| None)
            }
            Name::ExtCodeSize => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_ext_code::size(
                    context,
                    Some(arguments[0].into_int_value()),
                )
                .map(Some)
            }
            Name::ExtCodeHash => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                revive_llvm_context::polkavm_evm_ext_code::hash(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }

            Name::Return => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_return::r#return(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::Revert => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_return::revert(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::Stop => revive_llvm_context::polkavm_evm_return::stop(context).map(|_| None),
            Name::Invalid => {
                revive_llvm_context::polkavm_evm_return::invalid(context).map(|_| None)
            }

            Name::Log0 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                revive_llvm_context::polkavm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    [],
                )
                .map(|_| None)
            }
            Name::Log1 => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    [arguments[2]],
                )
                .map(|_| None)
            }
            Name::Log2 => {
                let arguments = self.pop_arguments_llvm::<D, 4>(context)?;
                revive_llvm_context::polkavm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    [arguments[2], arguments[3]],
                )
                .map(|_| None)
            }
            Name::Log3 => {
                let arguments = self.pop_arguments_llvm::<D, 5>(context)?;
                revive_llvm_context::polkavm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    [arguments[2], arguments[3], arguments[4]],
                )
                .map(|_| None)
            }
            Name::Log4 => {
                let arguments = self.pop_arguments_llvm::<D, 6>(context)?;
                revive_llvm_context::polkavm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    [arguments[2], arguments[3], arguments[4], arguments[5]],
                )
                .map(|_| None)
            }

            Name::Call => {
                let arguments = self.pop_arguments::<D, 7>(context)?;

                let gas = arguments[0].access(context)?.into_int_value();
                let address = arguments[1].access(context)?.into_int_value();
                let value = arguments[2].access(context)?.into_int_value();
                let input_offset = arguments[3].access(context)?.into_int_value();
                let input_size = arguments[4].access(context)?.into_int_value();
                let output_offset = arguments[5].access(context)?.into_int_value();
                let output_size = arguments[6].access(context)?.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                revive_llvm_context::polkavm_evm_call::call(
                    context,
                    gas,
                    address,
                    Some(value),
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                    false,
                )
                .map(Some)
            }
            Name::StaticCall => {
                let arguments = self.pop_arguments::<D, 6>(context)?;

                let gas = arguments[0].access(context)?.into_int_value();
                let address = arguments[1].access(context)?.into_int_value();
                let input_offset = arguments[2].access(context)?.into_int_value();
                let input_size = arguments[3].access(context)?.into_int_value();
                let output_offset = arguments[4].access(context)?.into_int_value();
                let output_size = arguments[5].access(context)?.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                revive_llvm_context::polkavm_evm_call::call(
                    context,
                    gas,
                    address,
                    None,
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                    true,
                )
                .map(Some)
            }
            Name::DelegateCall => {
                let arguments = self.pop_arguments::<D, 6>(context)?;

                let gas = arguments[0].access(context)?.into_int_value();
                let address = arguments[1].access(context)?.into_int_value();
                let input_offset = arguments[2].access(context)?.into_int_value();
                let input_size = arguments[3].access(context)?.into_int_value();
                let output_offset = arguments[4].access(context)?.into_int_value();
                let output_size = arguments[5].access(context)?.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                revive_llvm_context::polkavm_evm_call::delegate_call(
                    context,
                    gas,
                    address,
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                )
                .map(Some)
            }

            Name::Create => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;

                let value = arguments[0].into_int_value();
                let input_offset = arguments[1].into_int_value();
                let input_length = arguments[2].into_int_value();

                revive_llvm_context::polkavm_evm_create::create(
                    context,
                    value,
                    input_offset,
                    input_length,
                    None,
                )
                .map(Some)
            }
            Name::Create2 => {
                let arguments = self.pop_arguments_llvm::<D, 4>(context)?;

                let value = arguments[0].into_int_value();
                let input_offset = arguments[1].into_int_value();
                let input_length = arguments[2].into_int_value();
                let salt = arguments[3].into_int_value();

                revive_llvm_context::polkavm_evm_create::create(
                    context,
                    value,
                    input_offset,
                    input_length,
                    Some(salt),
                )
                .map(Some)
            }
            Name::DataOffset => {
                let mut arguments = self.pop_arguments::<D, 1>(context)?;

                let identifier = arguments[0].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `dataoffset` object identifier is missing", location)
                })?;

                revive_llvm_context::polkavm_evm_create::contract_hash(context, identifier)
                    .and_then(|argument| argument.access(context))
                    .map(Some)
            }
            Name::DataSize => {
                let mut arguments = self.pop_arguments::<D, 1>(context)?;

                let identifier = arguments[0].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `dataoffset` object identifier is missing", location)
                })?;

                revive_llvm_context::polkavm_evm_create::header_size(context, identifier)
                    .and_then(|argument| argument.access(context))
                    .map(Some)
            }
            Name::DataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                revive_llvm_context::polkavm_evm_memory::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }

            Name::LinkerSymbol => {
                let mut arguments = self.pop_arguments::<D, 1>(context)?;
                let path = arguments[0].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} Linker symbol literal is missing", location)
                })?;

                Ok(Some(
                    context
                        .resolve_library(path.as_str())?
                        .as_basic_value_enum(),
                ))
            }
            Name::MemoryGuard => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                Ok(Some(arguments[0]))
            }

            Name::Address => {
                revive_llvm_context::polkavm_evm_contract_context::address(context).map(Some)
            }
            Name::Caller => {
                revive_llvm_context::polkavm_evm_contract_context::caller(context).map(Some)
            }
            Name::CallValue => revive_llvm_context::polkavm_evm_ether_gas::value(context).map(Some),
            Name::Gas => revive_llvm_context::polkavm_evm_ether_gas::gas(context).map(Some),
            Name::Balance => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;

                let address = arguments[0].into_int_value();
                revive_llvm_context::polkavm_evm_ether_gas::balance(context, address).map(Some)
            }
            Name::SelfBalance => {
                revive_llvm_context::polkavm_evm_ether_gas::self_balance(context).map(Some)
            }

            Name::GasLimit => {
                revive_llvm_context::polkavm_evm_contract_context::gas_limit(context).map(Some)
            }
            Name::GasPrice => {
                revive_llvm_context::polkavm_evm_contract_context::gas_price(context).map(Some)
            }
            Name::Origin => {
                revive_llvm_context::polkavm_evm_contract_context::origin(context).map(Some)
            }
            Name::ChainId => {
                revive_llvm_context::polkavm_evm_contract_context::chain_id(context).map(Some)
            }
            Name::Timestamp => {
                revive_llvm_context::polkavm_evm_contract_context::block_timestamp(context)
                    .map(Some)
            }
            Name::Number => {
                revive_llvm_context::polkavm_evm_contract_context::block_number(context).map(Some)
            }
            Name::BlockHash => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                let index = arguments[0].into_int_value();

                revive_llvm_context::polkavm_evm_contract_context::block_hash(context, index)
                    .map(Some)
            }
            Name::BlobHash => {
                let _arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                anyhow::bail!(
                    "{} The `BLOBHASH` instruction is not supported in revive",
                    location
                );
            }
            Name::Difficulty | Name::Prevrandao => {
                revive_llvm_context::polkavm_evm_contract_context::difficulty(context).map(Some)
            }
            Name::CoinBase => {
                revive_llvm_context::polkavm_evm_contract_context::coinbase(context).map(Some)
            }
            Name::BaseFee => {
                revive_llvm_context::polkavm_evm_contract_context::basefee(context).map(Some)
            }
            Name::BlobBaseFee => {
                anyhow::bail!(
                    "{} The `BLOBBASEFEE` instruction is not supported in revive",
                    location
                );
            }
            Name::MSize => revive_llvm_context::polkavm_evm_memory::msize(context).map(Some),

            Name::Verbatim {
                input_size,
                output_size,
            } => verbatim::verbatim(context, &mut self, input_size, output_size),

            Name::CallCode => {
                let _arguments = self.pop_arguments_llvm::<D, 7>(context)?;
                anyhow::bail!("{} The `CALLCODE` instruction is not supported", location)
            }
            Name::Pc => anyhow::bail!("{} The `PC` instruction is not supported", location),
            Name::ExtCodeCopy => {
                let _arguments = self.pop_arguments_llvm::<D, 4>(context)?;
                anyhow::bail!(
                    "{} The `EXTCODECOPY` instruction is not supported",
                    location
                )
            }
            Name::SelfDestruct => {
                let _arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                anyhow::bail!(
                    "{} The `SELFDESTRUCT` instruction is not supported",
                    location
                )
            }
        }
    }

    /// Pops the specified number of arguments, converted into their LLVM values.
    fn pop_arguments_llvm<'ctx, D, const N: usize>(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    ) -> anyhow::Result<[inkwell::values::BasicValueEnum<'ctx>; N]>
    where
        D: revive_llvm_context::PolkaVMDependency + Clone,
    {
        let mut arguments = Vec::with_capacity(N);
        for (index, expression) in self.arguments.drain(0..N).rev().enumerate() {
            let name = format!("arg_{index}");
            let pointer = context.build_alloca(context.word_type(), &name);
            arguments.push(
                expression
                    .into_llvm(&[(name, pointer)], context)?
                    .expect("Always exists")
                    .access(context)?,
            );
        }
        arguments.reverse();

        Ok(arguments.try_into().expect("Always successful"))
    }

    /// Pops the specified number of arguments.
    fn pop_arguments<'ctx, D, const N: usize>(
        &mut self,
        context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    ) -> anyhow::Result<[revive_llvm_context::PolkaVMArgument<'ctx>; N]>
    where
        D: revive_llvm_context::PolkaVMDependency + Clone,
    {
        let mut arguments = Vec::with_capacity(N);
        for (index, expression) in self.arguments.drain(0..N).rev().enumerate() {
            let name = format!("arg_{index}");
            let pointer = context.build_alloca(context.word_type(), &name);
            arguments.push(
                expression
                    .into_llvm(&[(name, pointer)], context)?
                    .expect("Always exists"),
            );
        }
        arguments.reverse();

        Ok(arguments.try_into().expect("Always successful"))
    }
}
