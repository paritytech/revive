//!
//! The function call subexpression.
//!

pub mod name;
pub mod verbatim;

use std::collections::HashSet;

use inkwell::values::BasicValue;
use serde::Deserialize;
use serde::Serialize;

use crate::yul::error::Error;
use crate::yul::lexer::token::lexeme::literal::Literal as LexicalLiteral;
use crate::yul::lexer::token::lexeme::symbol::Symbol;
use crate::yul::lexer::token::lexeme::Lexeme;
use crate::yul::lexer::token::location::Location;
use crate::yul::lexer::token::Token;
use crate::yul::lexer::Lexer;
use crate::yul::parser::error::Error as ParserError;
use crate::yul::parser::statement::expression::literal::Literal;
use crate::yul::parser::statement::expression::Expression;

use self::name::Name;

///
/// The Yul function call subexpression.
///
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
    ///
    /// The element parser.
    ///
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;

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

    ///
    /// Get the list of missing deployable libraries.
    ///
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

    ///
    /// Converts the function call into an LLVM value.
    ///
    pub fn into_llvm<'ctx, D>(
        mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>>
    where
        D: era_compiler_llvm_context::EraVMDependency + Clone,
    {
        let location = self.location;

        match self.name {
            Name::UserDefined(name)
                if name.starts_with(
                    era_compiler_llvm_context::EraVMFunction::ZKSYNC_NEAR_CALL_ABI_PREFIX,
                ) && context.is_system_mode() =>
            {
                unimplemented!();
            }
            Name::UserDefined(name) => {
                let mut values = Vec::with_capacity(self.arguments.len());
                for argument in self.arguments.into_iter().rev() {
                    let value = argument.into_llvm(context)?.expect("Always exists").value;
                    values.push(value);
                }
                values.reverse();
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

                let return_value = context.build_invoke(
                    function.borrow().declaration(),
                    values.as_slice(),
                    format!("{name}_call").as_str(),
                );

                Ok(return_value)
            }

            Name::Add => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::addition(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Sub => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::subtraction(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Mul => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::multiplication(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Div => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::division(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Mod => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::remainder(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Sdiv => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::division_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Smod => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_arithmetic::remainder_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::Lt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::ULT,
                )
                .map(Some)
            }
            Name::Gt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::UGT,
                )
                .map(Some)
            }
            Name::Eq => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            Name::IsZero => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    context.field_const(0),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            Name::Slt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SLT,
                )
                .map(Some)
            }
            Name::Sgt => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SGT,
                )
                .map(Some)
            }

            Name::Or => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::or(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Xor => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Not => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    context.field_type().const_all_ones(),
                )
                .map(Some)
            }
            Name::And => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::and(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Shl => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::shift_left(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Shr => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::shift_right(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Sar => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::shift_right_arithmetic(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::Byte => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_bitwise::byte(
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
                era_compiler_llvm_context::eravm_evm_math::add_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            Name::MulMod => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                era_compiler_llvm_context::eravm_evm_math::mul_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            Name::Exp => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_math::exponent(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            Name::SignExtend => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_math::sign_extend(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::Keccak256 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_crypto::sha3(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            Name::MLoad => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_memory::load(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            Name::MStore => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_memory::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::MStore8 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_memory::store_byte(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::MCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                let destination = era_compiler_llvm_context::EraVMPointer::new_with_offset(
                    context,
                    era_compiler_llvm_context::EraVMAddressSpace::Heap,
                    context.byte_type(),
                    arguments[0].into_int_value(),
                    "mcopy_destination",
                );
                let source = era_compiler_llvm_context::EraVMPointer::new_with_offset(
                    context,
                    era_compiler_llvm_context::EraVMAddressSpace::Heap,
                    context.byte_type(),
                    arguments[1].into_int_value(),
                    "mcopy_source",
                );

                context.build_memcpy(
                    context.intrinsics().memory_copy,
                    destination,
                    source,
                    arguments[2].into_int_value(),
                    "mcopy_size",
                );
                Ok(None)
            }

            Name::SLoad => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_storage::load(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            Name::SStore => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_storage::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::TLoad => {
                let _arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                anyhow::bail!(
                    "{} The `TLOAD` instruction is not supported until zkVM v1.5.0",
                    location
                );
            }
            Name::TStore => {
                let _arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                anyhow::bail!(
                    "{} The `TSTORE` instruction is not supported until zkVM v1.5.0",
                    location
                );
            }
            Name::LoadImmutable => todo!(),
            Name::SetImmutable => {
                let mut arguments = self.pop_arguments::<D, 3>(context)?;
                let key = arguments[1].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `load_immutable` literal is missing", location)
                })?;

                if key.as_str() == "library_deploy_address" {
                    return Ok(None);
                }

                let offset = context.solidity_mut().allocate_immutable(key.as_str());

                let index = context.field_const(offset as u64);
                let value = arguments[2].value.into_int_value();
                era_compiler_llvm_context::eravm_evm_immutable::store(context, index, value)
                    .map(|_| None)
            }

            Name::CallDataLoad => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;

                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    era_compiler_llvm_context::EraVMCodeType::Deploy => {
                        Ok(Some(context.field_const(0).as_basic_value_enum()))
                    }
                    era_compiler_llvm_context::EraVMCodeType::Runtime => {
                        era_compiler_llvm_context::eravm_evm_calldata::load(
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
                    era_compiler_llvm_context::EraVMCodeType::Deploy => {
                        Ok(Some(context.field_const(0).as_basic_value_enum()))
                    }
                    era_compiler_llvm_context::EraVMCodeType::Runtime => {
                        era_compiler_llvm_context::eravm_evm_calldata::size(context).map(Some)
                    }
                }
            }
            Name::CallDataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;

                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    era_compiler_llvm_context::EraVMCodeType::Deploy => {
                        let calldata_size =
                            era_compiler_llvm_context::eravm_evm_calldata::size(context)?;

                        era_compiler_llvm_context::eravm_evm_calldata::copy(
                            context,
                            arguments[0].into_int_value(),
                            calldata_size.into_int_value(),
                            arguments[2].into_int_value(),
                        )
                        .map(|_| None)
                    }
                    era_compiler_llvm_context::EraVMCodeType::Runtime => {
                        era_compiler_llvm_context::eravm_evm_calldata::copy(
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
                    era_compiler_llvm_context::EraVMCodeType::Deploy => {
                        era_compiler_llvm_context::eravm_evm_calldata::size(context).map(Some)
                    }
                    era_compiler_llvm_context::EraVMCodeType::Runtime => {
                        let code_source =
                            era_compiler_llvm_context::eravm_general::code_source(context)?;
                        era_compiler_llvm_context::eravm_evm_ext_code::size(
                            context,
                            code_source.into_int_value(),
                        )
                        .map(Some)
                    }
                }
            }
            Name::CodeCopy => {
                if let era_compiler_llvm_context::EraVMCodeType::Runtime = context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    anyhow::bail!(
                        "{} The `CODECOPY` instruction is not supported in the runtime code",
                        location,
                    );
                }

                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                era_compiler_llvm_context::eravm_evm_calldata::copy(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(|_| None)
            }
            Name::ReturnDataSize => {
                era_compiler_llvm_context::eravm_evm_return_data::size(context).map(Some)
            }
            Name::ReturnDataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                era_compiler_llvm_context::eravm_evm_return_data::copy(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(|_| None)
            }
            Name::ExtCodeSize => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_ext_code::size(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            Name::ExtCodeHash => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                era_compiler_llvm_context::eravm_evm_ext_code::hash(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }

            Name::Return => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_return::r#return(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::Revert => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_return::revert(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            Name::Stop => era_compiler_llvm_context::eravm_evm_return::stop(context).map(|_| None),
            Name::Invalid => {
                era_compiler_llvm_context::eravm_evm_return::invalid(context).map(|_| None)
            }

            Name::Log0 => {
                let arguments = self.pop_arguments_llvm::<D, 2>(context)?;
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    vec![],
                )
                .map(|_| None)
            }
            Name::Log1 => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2..]
                        .iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            Name::Log2 => {
                let arguments = self.pop_arguments_llvm::<D, 4>(context)?;
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2..]
                        .iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            Name::Log3 => {
                let arguments = self.pop_arguments_llvm::<D, 5>(context)?;
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2..]
                        .iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            Name::Log4 => {
                let arguments = self.pop_arguments_llvm::<D, 6>(context)?;
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2..]
                        .iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }

            Name::Call => {
                let arguments = self.pop_arguments::<D, 7>(context)?;

                let gas = arguments[0].value.into_int_value();
                let address = arguments[1].value.into_int_value();
                let value = arguments[2].value.into_int_value();
                let input_offset = arguments[3].value.into_int_value();
                let input_size = arguments[4].value.into_int_value();
                let output_offset = arguments[5].value.into_int_value();
                let output_size = arguments[6].value.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                era_compiler_llvm_context::eravm_evm_call::default(
                    context,
                    context.llvm_runtime().far_call,
                    gas,
                    address,
                    Some(value),
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                )
                .map(Some)
            }
            Name::StaticCall => {
                let arguments = self.pop_arguments::<D, 6>(context)?;

                let gas = arguments[0].value.into_int_value();
                let address = arguments[1].value.into_int_value();
                let input_offset = arguments[2].value.into_int_value();
                let input_size = arguments[3].value.into_int_value();
                let output_offset = arguments[4].value.into_int_value();
                let output_size = arguments[5].value.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                era_compiler_llvm_context::eravm_evm_call::default(
                    context,
                    context.llvm_runtime().static_call,
                    gas,
                    address,
                    None,
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                )
                .map(Some)
            }
            Name::DelegateCall => {
                let arguments = self.pop_arguments::<D, 6>(context)?;

                let gas = arguments[0].value.into_int_value();
                let address = arguments[1].value.into_int_value();
                let input_offset = arguments[2].value.into_int_value();
                let input_size = arguments[3].value.into_int_value();
                let output_offset = arguments[4].value.into_int_value();
                let output_size = arguments[5].value.into_int_value();

                let simulation_address: Vec<Option<num::BigUint>> = arguments
                    .into_iter()
                    .map(|mut argument| argument.constant.take())
                    .collect();

                era_compiler_llvm_context::eravm_evm_call::default(
                    context,
                    context.llvm_runtime().delegate_call,
                    gas,
                    address,
                    None,
                    input_offset,
                    input_size,
                    output_offset,
                    output_size,
                    simulation_address,
                )
                .map(Some)
            }

            Name::Create | Name::ZkCreate => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;

                let value = arguments[0].into_int_value();
                let input_offset = arguments[1].into_int_value();
                let input_length = arguments[2].into_int_value();

                era_compiler_llvm_context::eravm_evm_create::create(
                    context,
                    value,
                    input_offset,
                    input_length,
                )
                .map(Some)
            }
            Name::Create2 | Name::ZkCreate2 => {
                let arguments = self.pop_arguments_llvm::<D, 4>(context)?;

                let value = arguments[0].into_int_value();
                let input_offset = arguments[1].into_int_value();
                let input_length = arguments[2].into_int_value();
                let salt = arguments[3].into_int_value();

                era_compiler_llvm_context::eravm_evm_create::create2(
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

                era_compiler_llvm_context::eravm_evm_create::contract_hash(context, identifier)
                    .map(|argument| Some(argument.value))
            }
            Name::DataSize => {
                let mut arguments = self.pop_arguments::<D, 1>(context)?;

                let identifier = arguments[0].original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `dataoffset` object identifier is missing", location)
                })?;

                era_compiler_llvm_context::eravm_evm_create::header_size(context, identifier)
                    .map(|argument| Some(argument.value))
            }
            Name::DataCopy => {
                let arguments = self.pop_arguments_llvm::<D, 3>(context)?;
                let offset = context.builder().build_int_add(
                    arguments[0].into_int_value(),
                    context.field_const(
                        (era_compiler_common::BYTE_LENGTH_X32
                            + era_compiler_common::BYTE_LENGTH_FIELD)
                            as u64,
                    ),
                    "datacopy_contract_hash_offset",
                )?;
                era_compiler_llvm_context::eravm_evm_memory::store(
                    context,
                    offset,
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

            Name::Address | Name::Caller => {
                Ok(Some(context.integer_const(256, 0).as_basic_value_enum()))
            }

            Name::CallValue => {
                era_compiler_llvm_context::eravm_evm_ether_gas::value(context).map(Some)
            }
            Name::Gas => era_compiler_llvm_context::eravm_evm_ether_gas::gas(context).map(Some),
            Name::Balance => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;

                let address = arguments[0].into_int_value();
                era_compiler_llvm_context::eravm_evm_ether_gas::balance(context, address).map(Some)
            }
            Name::SelfBalance => todo!(),

            Name::GasLimit => {
                era_compiler_llvm_context::eravm_evm_contract_context::gas_limit(context).map(Some)
            }
            Name::GasPrice => {
                era_compiler_llvm_context::eravm_evm_contract_context::gas_price(context).map(Some)
            }
            Name::Origin => {
                era_compiler_llvm_context::eravm_evm_contract_context::origin(context).map(Some)
            }
            Name::ChainId => {
                era_compiler_llvm_context::eravm_evm_contract_context::chain_id(context).map(Some)
            }
            Name::Timestamp => {
                era_compiler_llvm_context::eravm_evm_contract_context::block_timestamp(context)
                    .map(Some)
            }
            Name::Number => {
                era_compiler_llvm_context::eravm_evm_contract_context::block_number(context)
                    .map(Some)
            }
            Name::BlockHash => {
                let arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                let index = arguments[0].into_int_value();

                era_compiler_llvm_context::eravm_evm_contract_context::block_hash(context, index)
                    .map(Some)
            }
            Name::BlobHash => {
                let _arguments = self.pop_arguments_llvm::<D, 1>(context)?;
                anyhow::bail!(
                    "{} The `BLOBHASH` instruction is not supported until zkVM v1.5.0",
                    location
                );
            }
            Name::Difficulty | Name::Prevrandao => {
                era_compiler_llvm_context::eravm_evm_contract_context::difficulty(context).map(Some)
            }
            Name::CoinBase => {
                era_compiler_llvm_context::eravm_evm_contract_context::coinbase(context).map(Some)
            }
            Name::BaseFee => {
                era_compiler_llvm_context::eravm_evm_contract_context::basefee(context).map(Some)
            }
            Name::BlobBaseFee => {
                anyhow::bail!(
                    "{} The `BLOBBASEFEE` instruction is not supported until zkVM v1.5.0",
                    location
                );
            }
            Name::MSize => {
                era_compiler_llvm_context::eravm_evm_contract_context::msize(context).map(Some)
            }

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

            Name::ZkToL1 => {
                let [is_first, in_0, in_1] = self.pop_arguments_llvm::<D, 3>(context)?;

                era_compiler_llvm_context::eravm_general::to_l1(
                    context,
                    is_first.into_int_value(),
                    in_0.into_int_value(),
                    in_1.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkCodeSource => {
                era_compiler_llvm_context::eravm_general::code_source(context).map(Some)
            }
            Name::ZkPrecompile => {
                let [in_0, in_1] = self.pop_arguments_llvm::<D, 2>(context)?;

                era_compiler_llvm_context::eravm_general::precompile(
                    context,
                    in_0.into_int_value(),
                    in_1.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkMeta => era_compiler_llvm_context::eravm_general::meta(context).map(Some),
            Name::ZkSetContextU128 => {
                let [value] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_general::set_context_value(
                    context,
                    value.into_int_value(),
                )
                .map(|_| None)
            }
            Name::ZkSetPubdataPrice => {
                let [value] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_general::set_pubdata_price(
                    context,
                    value.into_int_value(),
                )
                .map(|_| None)
            }
            Name::ZkIncrementTxCounter => {
                era_compiler_llvm_context::eravm_general::increment_tx_counter(context)
                    .map(|_| None)
            }
            Name::ZkEventInitialize => {
                let [operand_1, operand_2] = self.pop_arguments_llvm::<D, 2>(context)?;

                era_compiler_llvm_context::eravm_general::event(
                    context,
                    operand_1.into_int_value(),
                    operand_2.into_int_value(),
                    true,
                )
                .map(|_| None)
            }
            Name::ZkEventWrite => {
                let [operand_1, operand_2] = self.pop_arguments_llvm::<D, 2>(context)?;

                era_compiler_llvm_context::eravm_general::event(
                    context,
                    operand_1.into_int_value(),
                    operand_2.into_int_value(),
                    false,
                )
                .map(|_| None)
            }

            Name::ZkMimicCall => {
                let [address, abi_data, mimic] = self.pop_arguments_llvm::<D, 3>(context)?;

                era_compiler_llvm_context::eravm_call::mimic(
                    context,
                    context.llvm_runtime().mimic_call,
                    address.into_int_value(),
                    mimic.into_int_value(),
                    abi_data.as_basic_value_enum(),
                    vec![],
                )
                .map(Some)
            }
            Name::ZkSystemMimicCall => {
                let [address, abi_data, mimic, extra_value_1, extra_value_2] =
                    self.pop_arguments_llvm::<D, 5>(context)?;

                era_compiler_llvm_context::eravm_call::mimic(
                    context,
                    context.llvm_runtime().mimic_call,
                    address.into_int_value(),
                    mimic.into_int_value(),
                    abi_data.as_basic_value_enum(),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkMimicCallByRef => {
                let [address, mimic] = self.pop_arguments_llvm::<D, 2>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::mimic(
                    context,
                    context.llvm_runtime().mimic_call_byref,
                    address.into_int_value(),
                    mimic.into_int_value(),
                    abi_data,
                    vec![],
                )
                .map(Some)
            }
            Name::ZkSystemMimicCallByRef => {
                let [address, mimic, extra_value_1, extra_value_2] =
                    self.pop_arguments_llvm::<D, 4>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::mimic(
                    context,
                    context.llvm_runtime().mimic_call_byref,
                    address.into_int_value(),
                    mimic.into_int_value(),
                    abi_data,
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkRawCall => {
                let [address, abi_data, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 4>(context)?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().far_call,
                    address.into_int_value(),
                    abi_data.as_basic_value_enum(),
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkRawCallByRef => {
                let [address, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 3>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().far_call_byref,
                    address.into_int_value(),
                    abi_data,
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkSystemCall => {
                let [address, abi_data, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 6>(context)?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().far_call,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkSystemCallByRef => {
                let [address, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 5>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().far_call_byref,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkStaticRawCall => {
                let [address, abi_data, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 4>(context)?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().static_call,
                    address.into_int_value(),
                    abi_data.as_basic_value_enum(),
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkStaticRawCallByRef => {
                let [address, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 3>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().static_call_byref,
                    address.into_int_value(),
                    abi_data,
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkStaticSystemCall => {
                let [address, abi_data, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 6>(context)?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().static_call,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkStaticSystemCallByRef => {
                let [address, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 5>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().static_call_byref,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkDelegateRawCall => {
                let [address, abi_data, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 4>(context)?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().delegate_call,
                    address.into_int_value(),
                    abi_data.as_basic_value_enum(),
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkDelegateRawCallByRef => {
                let [address, output_offset, output_length] =
                    self.pop_arguments_llvm::<D, 3>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::raw_far(
                    context,
                    context.llvm_runtime().delegate_call_byref,
                    address.into_int_value(),
                    abi_data,
                    output_offset.into_int_value(),
                    output_length.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkDelegateSystemCall => {
                let [address, abi_data, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 6>(context)?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().delegate_call,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }
            Name::ZkDelegateSystemCallByRef => {
                let [address, extra_value_1, extra_value_2, extra_value_3, extra_value_4] =
                    self.pop_arguments_llvm::<D, 5>(context)?;
                let abi_data = context.get_global_value(
                    era_compiler_llvm_context::eravm_const::GLOBAL_ACTIVE_POINTER,
                )?;

                era_compiler_llvm_context::eravm_call::system(
                    context,
                    context.llvm_runtime().delegate_call_byref,
                    address.into_int_value(),
                    abi_data,
                    context.field_const(0),
                    context.field_const(0),
                    vec![
                        extra_value_1.into_int_value(),
                        extra_value_2.into_int_value(),
                        extra_value_3.into_int_value(),
                        extra_value_4.into_int_value(),
                    ],
                )
                .map(Some)
            }

            Name::ZkLoadCalldataIntoActivePtr => {
                era_compiler_llvm_context::eravm_abi::calldata_ptr_to_active(context).map(|_| None)
            }
            Name::ZkLoadReturndataIntoActivePtr => {
                era_compiler_llvm_context::eravm_abi::return_data_ptr_to_active(context)
                    .map(|_| None)
            }
            Name::ZkPtrAddIntoActive => {
                let [offset] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_abi::active_ptr_add_assign(
                    context,
                    offset.into_int_value(),
                )
                .map(|_| None)
            }
            Name::ZkPtrShrinkIntoActive => {
                let [offset] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_abi::active_ptr_shrink_assign(
                    context,
                    offset.into_int_value(),
                )
                .map(|_| None)
            }
            Name::ZkPtrPackIntoActive => {
                let [data] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_abi::active_ptr_pack_assign(
                    context,
                    data.into_int_value(),
                )
                .map(|_| None)
            }

            Name::ZkMultiplicationHigh => {
                let [operand_1, operand_2] = self.pop_arguments_llvm::<D, 2>(context)?;

                era_compiler_llvm_context::eravm_math::multiplication_512(
                    context,
                    operand_1.into_int_value(),
                    operand_2.into_int_value(),
                )
                .map(Some)
            }

            Name::ZkGlobalLoad => {
                let [mut key] = self.pop_arguments::<D, 1>(context)?;
                let key = key.original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `$zk_global_load` literal is missing", location)
                })?;

                context.get_global_value(key.as_str()).map(Some)
            }
            Name::ZkGlobalExtraAbiData => {
                let [index] = self.pop_arguments_llvm::<D, 1>(context)?;

                era_compiler_llvm_context::eravm_abi::get_extra_abi_data(
                    context,
                    index.into_int_value(),
                )
                .map(Some)
            }
            Name::ZkGlobalStore => {
                let [mut key, value] = self.pop_arguments::<D, 2>(context)?;
                let key = key.original.take().ok_or_else(|| {
                    anyhow::anyhow!("{} `$zk_global_store` literal is missing", location)
                })?;
                let value = value.value.into_int_value();

                context.set_global(
                    key.as_str(),
                    context.field_type(),
                    era_compiler_llvm_context::EraVMAddressSpace::Stack,
                    value,
                );
                Ok(None)
            }
        }
    }

    ///
    /// Pops the specified number of arguments, converted into their LLVM values.
    ///
    fn pop_arguments_llvm<'ctx, D, const N: usize>(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> anyhow::Result<[inkwell::values::BasicValueEnum<'ctx>; N]>
    where
        D: era_compiler_llvm_context::EraVMDependency + Clone,
    {
        let mut arguments = Vec::with_capacity(N);
        for expression in self.arguments.drain(0..N).rev() {
            arguments.push(expression.into_llvm(context)?.expect("Always exists").value);
        }
        arguments.reverse();

        Ok(arguments.try_into().expect("Always successful"))
    }

    ///
    /// Pops the specified number of arguments.
    ///
    fn pop_arguments<'ctx, D, const N: usize>(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> anyhow::Result<[era_compiler_llvm_context::EraVMArgument<'ctx>; N]>
    where
        D: era_compiler_llvm_context::EraVMDependency + Clone,
    {
        let mut arguments = Vec::with_capacity(N);
        for expression in self.arguments.drain(0..N).rev() {
            arguments.push(expression.into_llvm(context)?.expect("Always exists"));
        }
        arguments.reverse();

        Ok(arguments.try_into().expect("Always successful"))
    }
}
