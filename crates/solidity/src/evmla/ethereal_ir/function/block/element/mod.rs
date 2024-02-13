//!
//! The Ethereal IR block element.
//!

pub mod stack;

use inkwell::values::BasicValue;

use crate::evmla::assembly::instruction::codecopy;
use crate::evmla::assembly::instruction::name::Name as InstructionName;
use crate::evmla::assembly::instruction::Instruction;

use self::stack::element::Element as StackElement;
use self::stack::Stack;

///
/// The Ethereal IR block element.
///
#[derive(Debug, Clone)]
pub struct Element {
    /// The Solidity compiler version.
    pub solc_version: semver::Version,
    /// The instruction.
    pub instruction: Instruction,
    /// The stack data.
    pub stack: Stack,
    /// The stack input.
    pub stack_input: Stack,
    /// The stack output.
    pub stack_output: Stack,
}

impl Element {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(solc_version: semver::Version, instruction: Instruction) -> Self {
        let input_size = instruction.input_size(&solc_version);
        let output_size = instruction.output_size();

        Self {
            solc_version,
            instruction,
            stack: Stack::new(),
            stack_input: Stack::with_capacity(input_size),
            stack_output: Stack::with_capacity(output_size),
        }
    }

    ///
    /// Pops the specified number of arguments, converted into their LLVM values.
    ///
    fn pop_arguments_llvm<'ctx, D>(
        &mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'ctx, D>,
    ) -> Vec<inkwell::values::BasicValueEnum<'ctx>>
    where
        D: era_compiler_llvm_context::EraVMDependency + Clone,
    {
        let input_size = self.instruction.input_size(&context.evmla().version);
        let output_size = self.instruction.output_size();
        let mut arguments = Vec::with_capacity(input_size);
        for index in 0..input_size {
            let pointer = context.evmla().stack
                [self.stack.elements.len() + input_size - output_size - 1 - index]
                .to_llvm()
                .into_pointer_value();
            let value = context.build_load(
                era_compiler_llvm_context::EraVMPointer::new_stack_field(context, pointer),
                format!("argument_{index}").as_str(),
            );
            arguments.push(value);
        }
        arguments
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for Element
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn into_llvm(
        mut self,
        context: &mut era_compiler_llvm_context::EraVMContext<'_, D>,
    ) -> anyhow::Result<()> {
        let mut original = self.instruction.value.clone();

        let result = match self.instruction.name.clone() {
            InstructionName::PUSH
            | InstructionName::PUSH1
            | InstructionName::PUSH2
            | InstructionName::PUSH3
            | InstructionName::PUSH4
            | InstructionName::PUSH5
            | InstructionName::PUSH6
            | InstructionName::PUSH7
            | InstructionName::PUSH8
            | InstructionName::PUSH9
            | InstructionName::PUSH10
            | InstructionName::PUSH11
            | InstructionName::PUSH12
            | InstructionName::PUSH13
            | InstructionName::PUSH14
            | InstructionName::PUSH15
            | InstructionName::PUSH16
            | InstructionName::PUSH17
            | InstructionName::PUSH18
            | InstructionName::PUSH19
            | InstructionName::PUSH20
            | InstructionName::PUSH21
            | InstructionName::PUSH22
            | InstructionName::PUSH23
            | InstructionName::PUSH24
            | InstructionName::PUSH25
            | InstructionName::PUSH26
            | InstructionName::PUSH27
            | InstructionName::PUSH28
            | InstructionName::PUSH29
            | InstructionName::PUSH30
            | InstructionName::PUSH31
            | InstructionName::PUSH32 => crate::evmla::assembly::instruction::stack::push(
                context,
                self.instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?,
            )
            .map(Some),
            InstructionName::PUSH_Tag => crate::evmla::assembly::instruction::stack::push_tag(
                context,
                self.instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?,
            )
            .map(Some),
            InstructionName::PUSH_ContractHash => {
                era_compiler_llvm_context::eravm_evm_create::contract_hash(
                    context,
                    self.instruction
                        .value
                        .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?,
                )
                .map(|argument| Some(argument.value))
            }
            InstructionName::PUSH_ContractHashSize => {
                era_compiler_llvm_context::eravm_evm_create::header_size(
                    context,
                    self.instruction
                        .value
                        .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?,
                )
                .map(|argument| Some(argument.value))
            }
            InstructionName::PUSHLIB => {
                let path = self
                    .instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?;

                Ok(Some(
                    context
                        .resolve_library(path.as_str())?
                        .as_basic_value_enum(),
                ))
            }
            InstructionName::PUSH_Data => {
                let value = self
                    .instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?;

                if value.len() > era_compiler_common::BYTE_LENGTH_FIELD * 2 {
                    Ok(Some(context.field_const(0).as_basic_value_enum()))
                } else {
                    crate::evmla::assembly::instruction::stack::push(context, value).map(Some)
                }
            }
            InstructionName::PUSHDEPLOYADDRESS => todo!(),

            InstructionName::DUP1 => crate::evmla::assembly::instruction::stack::dup(
                context,
                1,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP2 => crate::evmla::assembly::instruction::stack::dup(
                context,
                2,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP3 => crate::evmla::assembly::instruction::stack::dup(
                context,
                3,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP4 => crate::evmla::assembly::instruction::stack::dup(
                context,
                4,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP5 => crate::evmla::assembly::instruction::stack::dup(
                context,
                5,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP6 => crate::evmla::assembly::instruction::stack::dup(
                context,
                6,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP7 => crate::evmla::assembly::instruction::stack::dup(
                context,
                7,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP8 => crate::evmla::assembly::instruction::stack::dup(
                context,
                8,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP9 => crate::evmla::assembly::instruction::stack::dup(
                context,
                9,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP10 => crate::evmla::assembly::instruction::stack::dup(
                context,
                10,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP11 => crate::evmla::assembly::instruction::stack::dup(
                context,
                11,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP12 => crate::evmla::assembly::instruction::stack::dup(
                context,
                12,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP13 => crate::evmla::assembly::instruction::stack::dup(
                context,
                13,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP14 => crate::evmla::assembly::instruction::stack::dup(
                context,
                14,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP15 => crate::evmla::assembly::instruction::stack::dup(
                context,
                15,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),
            InstructionName::DUP16 => crate::evmla::assembly::instruction::stack::dup(
                context,
                16,
                self.stack.elements.len(),
                &mut original,
            )
            .map(Some),

            InstructionName::SWAP1 => crate::evmla::assembly::instruction::stack::swap(
                context,
                1,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP2 => crate::evmla::assembly::instruction::stack::swap(
                context,
                2,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP3 => crate::evmla::assembly::instruction::stack::swap(
                context,
                3,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP4 => crate::evmla::assembly::instruction::stack::swap(
                context,
                4,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP5 => crate::evmla::assembly::instruction::stack::swap(
                context,
                5,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP6 => crate::evmla::assembly::instruction::stack::swap(
                context,
                6,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP7 => crate::evmla::assembly::instruction::stack::swap(
                context,
                7,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP8 => crate::evmla::assembly::instruction::stack::swap(
                context,
                8,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP9 => crate::evmla::assembly::instruction::stack::swap(
                context,
                9,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP10 => crate::evmla::assembly::instruction::stack::swap(
                context,
                10,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP11 => crate::evmla::assembly::instruction::stack::swap(
                context,
                11,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP12 => crate::evmla::assembly::instruction::stack::swap(
                context,
                12,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP13 => crate::evmla::assembly::instruction::stack::swap(
                context,
                13,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP14 => crate::evmla::assembly::instruction::stack::swap(
                context,
                14,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP15 => crate::evmla::assembly::instruction::stack::swap(
                context,
                15,
                self.stack.elements.len(),
            )
            .map(|_| None),
            InstructionName::SWAP16 => crate::evmla::assembly::instruction::stack::swap(
                context,
                16,
                self.stack.elements.len(),
            )
            .map(|_| None),

            InstructionName::POP => {
                crate::evmla::assembly::instruction::stack::pop(context).map(|_| None)
            }

            InstructionName::Tag => {
                let destination: num::BigUint = self
                    .instruction
                    .value
                    .expect("Always exists")
                    .parse()
                    .expect("Always valid");

                crate::evmla::assembly::instruction::jump::unconditional(
                    context,
                    destination,
                    self.stack.hash(),
                )
                .map(|_| None)
            }
            InstructionName::JUMP => {
                let destination = self.stack_input.pop_tag()?;

                crate::evmla::assembly::instruction::jump::unconditional(
                    context,
                    destination,
                    self.stack.hash(),
                )
                .map(|_| None)
            }
            InstructionName::JUMPI => {
                let destination = self.stack_input.pop_tag()?;
                let _condition = self.stack_input.pop();

                crate::evmla::assembly::instruction::jump::conditional(
                    context,
                    destination,
                    self.stack.hash(),
                    self.stack.elements.len(),
                )
                .map(|_| None)
            }
            InstructionName::JUMPDEST => Ok(None),

            InstructionName::ADD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::addition(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SUB => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::subtraction(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::MUL => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::multiplication(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::DIV => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::division(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::MOD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::remainder(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SDIV => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::division_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SMOD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_arithmetic::remainder_signed(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            InstructionName::LT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::ULT,
                )
                .map(Some)
            }
            InstructionName::GT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::UGT,
                )
                .map(Some)
            }
            InstructionName::EQ => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            InstructionName::ISZERO => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    context.field_const(0),
                    inkwell::IntPredicate::EQ,
                )
                .map(Some)
            }
            InstructionName::SLT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SLT,
                )
                .map(Some)
            }
            InstructionName::SGT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_comparison::compare(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    inkwell::IntPredicate::SGT,
                )
                .map(Some)
            }

            InstructionName::OR => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::or(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::XOR => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::NOT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::xor(
                    context,
                    arguments[0].into_int_value(),
                    context.field_type().const_all_ones(),
                )
                .map(Some)
            }
            InstructionName::AND => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::and(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SHL => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::shift_left(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SHR => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::shift_right(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SAR => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::shift_right_arithmetic(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::BYTE => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_bitwise::byte(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            InstructionName::ADDMOD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_math::add_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::MULMOD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_math::mul_mod(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::EXP => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_math::exponent(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SIGNEXTEND => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_math::sign_extend(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            InstructionName::SHA3 | InstructionName::KECCAK256 => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_crypto::sha3(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(Some)
            }

            InstructionName::MLOAD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_memory::load(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::MSTORE => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_memory::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::MSTORE8 => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_memory::store_byte(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::MCOPY => {
                let arguments = self.pop_arguments_llvm(context);
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

            InstructionName::SLOAD => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_storage::load(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::SSTORE => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_storage::store(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::TLOAD => {
                let _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `TLOAD` instruction is not supported until zkVM v1.5.0");
            }
            InstructionName::TSTORE => {
                let _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `TSTORE` instruction is not supported until zkVM v1.5.0");
            }
            InstructionName::PUSHIMMUTABLE => {
                let key = self
                    .instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?;

                let offset = context
                    .solidity_mut()
                    .get_or_allocate_immutable(key.as_str());

                let index = context.field_const(offset as u64);
                era_compiler_llvm_context::eravm_evm_immutable::load(context, index).map(Some)
            }
            InstructionName::ASSIGNIMMUTABLE => {
                let mut arguments = self.pop_arguments_llvm(context);

                let key = self
                    .instruction
                    .value
                    .ok_or_else(|| anyhow::anyhow!("Instruction value missing"))?;

                let offset = context.solidity_mut().allocate_immutable(key.as_str());

                let index = context.field_const(offset as u64);
                let value = arguments.pop().expect("Always exists").into_int_value();
                era_compiler_llvm_context::eravm_evm_immutable::store(context, index, value)
                    .map(|_| None)
            }

            InstructionName::CALLDATALOAD => {
                match context
                    .code_type()
                    .ok_or_else(|| anyhow::anyhow!("The contract code part type is undefined"))?
                {
                    era_compiler_llvm_context::EraVMCodeType::Deploy => {
                        Ok(Some(context.field_const(0).as_basic_value_enum()))
                    }
                    era_compiler_llvm_context::EraVMCodeType::Runtime => {
                        let arguments = self.pop_arguments_llvm(context);
                        era_compiler_llvm_context::eravm_evm_calldata::load(
                            context,
                            arguments[0].into_int_value(),
                        )
                        .map(Some)
                    }
                }
            }
            InstructionName::CALLDATASIZE => {
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
            InstructionName::CALLDATACOPY => {
                let arguments = self.pop_arguments_llvm(context);

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
            InstructionName::CODESIZE => {
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
            InstructionName::CODECOPY => {
                let arguments = self.pop_arguments_llvm(context);

                let parent = context.module().get_name().to_str().expect("Always valid");
                let source = &self.stack_input.elements[1];
                let destination = &self.stack_input.elements[2];

                let library_marker: u64 = 0x0b;
                let library_flag: u64 = 0x73;

                match (source, destination) {
                    (_, StackElement::Constant(destination))
                        if destination == &num::BigUint::from(library_marker) =>
                    {
                        codecopy::library_marker(context, library_marker, library_flag)
                    }
                    (StackElement::Data(data), _) => {
                        codecopy::static_data(context, arguments[0].into_int_value(), data.as_str())
                    }
                    (StackElement::Path(source), _) if source != parent => codecopy::contract_hash(
                        context,
                        arguments[0].into_int_value(),
                        arguments[1].into_int_value(),
                    ),
                    _ => {
                        match context.code_type().ok_or_else(|| {
                            anyhow::anyhow!("The contract code part type is undefined")
                        })? {
                            era_compiler_llvm_context::EraVMCodeType::Deploy => {
                                era_compiler_llvm_context::eravm_evm_calldata::copy(
                                    context,
                                    arguments[0].into_int_value(),
                                    arguments[1].into_int_value(),
                                    arguments[2].into_int_value(),
                                )
                            }
                            era_compiler_llvm_context::EraVMCodeType::Runtime => {
                                let calldata_size =
                                    era_compiler_llvm_context::eravm_evm_calldata::size(context)?;
                                era_compiler_llvm_context::eravm_evm_calldata::copy(
                                    context,
                                    arguments[0].into_int_value(),
                                    calldata_size.into_int_value(),
                                    arguments[2].into_int_value(),
                                )
                            }
                        }
                    }
                }
                .map(|_| None)
            }
            InstructionName::PUSHSIZE => Ok(Some(context.field_const(0).as_basic_value_enum())),
            InstructionName::RETURNDATASIZE => {
                era_compiler_llvm_context::eravm_evm_return_data::size(context).map(Some)
            }
            InstructionName::RETURNDATACOPY => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_return_data::copy(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                    arguments[2].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::EXTCODESIZE => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_ext_code::size(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }
            InstructionName::EXTCODEHASH => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_ext_code::hash(
                    context,
                    arguments[0].into_int_value(),
                )
                .map(Some)
            }

            InstructionName::RETURN => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_return::r#return(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::REVERT => {
                let arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_return::revert(
                    context,
                    arguments[0].into_int_value(),
                    arguments[1].into_int_value(),
                )
                .map(|_| None)
            }
            InstructionName::STOP => {
                era_compiler_llvm_context::eravm_evm_return::stop(context).map(|_| None)
            }
            InstructionName::INVALID => {
                era_compiler_llvm_context::eravm_evm_return::invalid(context).map(|_| None)
            }

            InstructionName::LOG0 => {
                let mut arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments.remove(0).into_int_value(),
                    arguments.remove(0).into_int_value(),
                    arguments
                        .into_iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            InstructionName::LOG1 => {
                let mut arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments.remove(0).into_int_value(),
                    arguments.remove(0).into_int_value(),
                    arguments
                        .into_iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            InstructionName::LOG2 => {
                let mut arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments.remove(0).into_int_value(),
                    arguments.remove(0).into_int_value(),
                    arguments
                        .into_iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            InstructionName::LOG3 => {
                let mut arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments.remove(0).into_int_value(),
                    arguments.remove(0).into_int_value(),
                    arguments
                        .into_iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }
            InstructionName::LOG4 => {
                let mut arguments = self.pop_arguments_llvm(context);
                era_compiler_llvm_context::eravm_evm_event::log(
                    context,
                    arguments.remove(0).into_int_value(),
                    arguments.remove(0).into_int_value(),
                    arguments
                        .into_iter()
                        .map(|argument| argument.into_int_value())
                        .collect(),
                )
                .map(|_| None)
            }

            InstructionName::CALL => {
                let mut arguments = self.pop_arguments_llvm(context);

                let gas = arguments.remove(0).into_int_value();
                let address = arguments.remove(0).into_int_value();
                let value = arguments.remove(0).into_int_value();
                let input_offset = arguments.remove(0).into_int_value();
                let input_size = arguments.remove(0).into_int_value();
                let output_offset = arguments.remove(0).into_int_value();
                let output_size = arguments.remove(0).into_int_value();

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
                    vec![],
                )
                .map(Some)
            }
            InstructionName::STATICCALL => {
                let mut arguments = self.pop_arguments_llvm(context);

                let gas = arguments.remove(0).into_int_value();
                let address = arguments.remove(0).into_int_value();
                let input_offset = arguments.remove(0).into_int_value();
                let input_size = arguments.remove(0).into_int_value();
                let output_offset = arguments.remove(0).into_int_value();
                let output_size = arguments.remove(0).into_int_value();

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
                    vec![],
                )
                .map(Some)
            }
            InstructionName::DELEGATECALL => {
                let mut arguments = self.pop_arguments_llvm(context);

                let gas = arguments.remove(0).into_int_value();
                let address = arguments.remove(0).into_int_value();
                let input_offset = arguments.remove(0).into_int_value();
                let input_size = arguments.remove(0).into_int_value();
                let output_offset = arguments.remove(0).into_int_value();
                let output_size = arguments.remove(0).into_int_value();

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
                    vec![],
                )
                .map(Some)
            }

            InstructionName::CREATE | InstructionName::ZK_CREATE => {
                let arguments = self.pop_arguments_llvm(context);

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
            InstructionName::CREATE2 | InstructionName::ZK_CREATE2 => {
                let arguments = self.pop_arguments_llvm(context);

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

            InstructionName::ADDRESS => todo!(),
            InstructionName::CALLER => todo!(),

            InstructionName::CALLVALUE => {
                era_compiler_llvm_context::eravm_evm_ether_gas::value(context).map(Some)
            }
            InstructionName::GAS => {
                era_compiler_llvm_context::eravm_evm_ether_gas::gas(context).map(Some)
            }
            InstructionName::BALANCE => {
                let arguments = self.pop_arguments_llvm(context);

                let address = arguments[0].into_int_value();
                era_compiler_llvm_context::eravm_evm_ether_gas::balance(context, address).map(Some)
            }
            InstructionName::SELFBALANCE => todo!(),

            InstructionName::GASLIMIT => {
                era_compiler_llvm_context::eravm_evm_contract_context::gas_limit(context).map(Some)
            }
            InstructionName::GASPRICE => {
                era_compiler_llvm_context::eravm_evm_contract_context::gas_price(context).map(Some)
            }
            InstructionName::ORIGIN => {
                era_compiler_llvm_context::eravm_evm_contract_context::origin(context).map(Some)
            }
            InstructionName::CHAINID => {
                era_compiler_llvm_context::eravm_evm_contract_context::chain_id(context).map(Some)
            }
            InstructionName::TIMESTAMP => {
                era_compiler_llvm_context::eravm_evm_contract_context::block_timestamp(context)
                    .map(Some)
            }
            InstructionName::NUMBER => {
                era_compiler_llvm_context::eravm_evm_contract_context::block_number(context)
                    .map(Some)
            }
            InstructionName::BLOCKHASH => {
                let arguments = self.pop_arguments_llvm(context);
                let index = arguments[0].into_int_value();

                era_compiler_llvm_context::eravm_evm_contract_context::block_hash(context, index)
                    .map(Some)
            }
            InstructionName::BLOBHASH => {
                let _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `BLOBHASH` instruction is not supported until zkVM v1.5.0");
            }
            InstructionName::DIFFICULTY | InstructionName::PREVRANDAO => {
                era_compiler_llvm_context::eravm_evm_contract_context::difficulty(context).map(Some)
            }
            InstructionName::COINBASE => {
                era_compiler_llvm_context::eravm_evm_contract_context::coinbase(context).map(Some)
            }
            InstructionName::BASEFEE => {
                era_compiler_llvm_context::eravm_evm_contract_context::basefee(context).map(Some)
            }
            InstructionName::BLOBBASEFEE => {
                anyhow::bail!("The `BLOBBASEFEE` instruction is not supported until zkVM v1.5.0");
            }
            InstructionName::MSIZE => {
                era_compiler_llvm_context::eravm_evm_contract_context::msize(context).map(Some)
            }

            InstructionName::CALLCODE => {
                let mut _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `CALLCODE` instruction is not supported");
            }
            InstructionName::PC => {
                anyhow::bail!("The `PC` instruction is not supported");
            }
            InstructionName::EXTCODECOPY => {
                let _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `EXTCODECOPY` instruction is not supported");
            }
            InstructionName::SELFDESTRUCT => {
                let _arguments = self.pop_arguments_llvm(context);
                anyhow::bail!("The `SELFDESTRUCT` instruction is not supported");
            }

            InstructionName::RecursiveCall {
                name,
                entry_key,
                stack_hash,
                output_size,
                return_address,
                ..
            } => {
                let mut arguments = self.pop_arguments_llvm(context);
                arguments.pop();
                arguments.reverse();
                arguments.pop();

                let function = context
                    .get_function(format!("{name}_{entry_key}").as_str())
                    .expect("Always exists")
                    .borrow()
                    .declaration();
                let result = context.build_call(
                    function,
                    arguments.as_slice(),
                    format!("call_{}", name).as_str(),
                );
                match result {
                    Some(value) if value.is_int_value() => {
                        let pointer = context.evmla().stack
                            [self.stack.elements.len() - output_size]
                            .to_llvm()
                            .into_pointer_value();
                        context.build_store(
                            era_compiler_llvm_context::EraVMPointer::new_stack_field(
                                context, pointer,
                            ),
                            value,
                        );
                    }
                    Some(value) if value.is_struct_value() => {
                        let return_value = value.into_struct_value();
                        for index in 0..output_size {
                            let value = context
                                .builder()
                                .build_extract_value(
                                    return_value,
                                    index as u32,
                                    format!("return_value_element_{}", index).as_str(),
                                )
                                .expect("Always exists");
                            let pointer = era_compiler_llvm_context::EraVMPointer::new(
                                context.field_type(),
                                era_compiler_llvm_context::EraVMAddressSpace::Stack,
                                context.evmla().stack
                                    [self.stack.elements.len() - output_size + index]
                                    .to_llvm()
                                    .into_pointer_value(),
                            );
                            context.build_store(pointer, value);
                        }
                    }
                    Some(_) => {
                        panic!("Only integers and structures can be returned from Ethir functions")
                    }
                    None => {}
                }

                let return_block = context
                    .current_function()
                    .borrow()
                    .evmla()
                    .find_block(&return_address, &stack_hash)?;
                context.build_unconditional_branch(return_block.inner());
                return Ok(());
            }
            InstructionName::RecursiveReturn { .. } => {
                let mut arguments = self.pop_arguments_llvm(context);
                arguments.reverse();
                arguments.pop();

                match context.current_function().borrow().r#return() {
                    era_compiler_llvm_context::EraVMFunctionReturn::None => {}
                    era_compiler_llvm_context::EraVMFunctionReturn::Primitive { pointer } => {
                        assert_eq!(arguments.len(), 1);
                        context.build_store(pointer, arguments.remove(0));
                    }
                    era_compiler_llvm_context::EraVMFunctionReturn::Compound {
                        pointer, ..
                    } => {
                        for (index, argument) in arguments.into_iter().enumerate() {
                            let element_pointer = context.build_gep(
                                pointer,
                                &[
                                    context.field_const(0),
                                    context.integer_const(
                                        era_compiler_common::BIT_LENGTH_X32,
                                        index as u64,
                                    ),
                                ],
                                context.field_type(),
                                format!("return_value_pointer_element_{}", index).as_str(),
                            );
                            context.build_store(element_pointer, argument);
                        }
                    }
                }

                let return_block = context.current_function().borrow().return_block();
                context.build_unconditional_branch(return_block);
                Ok(None)
            }
        }?;

        if let Some(result) = result {
            let pointer = context.evmla().stack[self.stack.elements.len() - 1]
                .to_llvm()
                .into_pointer_value();
            context.build_store(
                era_compiler_llvm_context::EraVMPointer::new_stack_field(context, pointer),
                result,
            );
            context.evmla_mut().stack[self.stack.elements.len() - 1].original = original;
        }

        Ok(())
    }
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut stack = self.stack.to_owned();
        for _ in 0..self.stack_output.len() {
            let _ = stack.pop();
        }

        write!(f, "{:80}{}", self.instruction.to_string(), stack)?;
        if !self.stack_input.is_empty() {
            write!(f, " - {}", self.stack_input)?;
        }
        if !self.stack_output.is_empty() {
            write!(f, " + {}", self.stack_output)?;
        }
        Ok(())
    }
}
