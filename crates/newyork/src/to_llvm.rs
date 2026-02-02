//! LLVM code generation for the newyork IR.
//!
//! This module implements translation from newyork IR to LLVM IR via inkwell,
//! reusing the PolkaVM context infrastructure from revive-llvm-context.
//!
//! NOTE: This is a work-in-progress implementation. Many functions are stubbed
//! or simplified for initial development.

use std::collections::BTreeMap;

use inkwell::types::BasicType;
use inkwell::values::{BasicValue, BasicValueEnum, IntValue};
use num::ToPrimitive;
use revive_llvm_context::{PolkaVMArgument, PolkaVMContext};

use crate::ir::{
    BinOp, Block, Expr, Function, FunctionId, Object, Region, Statement, Type, UnaryOp, Value,
    ValueId,
};

/// Error type for LLVM codegen.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error("LLVM error: {0}")]
    Llvm(String),

    #[error("Undefined value: {0:?}")]
    UndefinedValue(ValueId),

    #[error("Undefined function: {0:?}")]
    UndefinedFunction(FunctionId),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

impl From<anyhow::Error> for CodegenError {
    fn from(err: anyhow::Error) -> Self {
        CodegenError::Llvm(err.to_string())
    }
}

/// Result type for codegen operations.
pub type Result<T> = std::result::Result<T, CodegenError>;

/// LLVM code generator for newyork IR.
pub struct LlvmCodegen<'ctx> {
    /// Value table: maps IR ValueId to LLVM value.
    values: BTreeMap<u32, BasicValueEnum<'ctx>>,
    /// Function table: maps IR FunctionId to function name.
    function_names: BTreeMap<u32, String>,
}

impl<'ctx> LlvmCodegen<'ctx> {
    /// Creates a new code generator.
    pub fn new() -> Self {
        LlvmCodegen {
            values: BTreeMap::new(),
            function_names: BTreeMap::new(),
        }
    }

    /// Gets an LLVM value by IR ValueId.
    fn get_value(&self, id: ValueId) -> Result<BasicValueEnum<'ctx>> {
        self.values
            .get(&id.0)
            .copied()
            .ok_or(CodegenError::UndefinedValue(id))
    }

    /// Stores an LLVM value for an IR ValueId.
    fn set_value(&mut self, id: ValueId, value: BasicValueEnum<'ctx>) {
        self.values.insert(id.0, value);
    }

    /// Translates an IR Value to LLVM value.
    fn translate_value(&self, value: &Value) -> Result<BasicValueEnum<'ctx>> {
        self.get_value(value.id)
    }

    /// Generates LLVM IR for a complete object.
    pub fn generate_object(
        &mut self,
        object: &Object,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        // First pass: declare all functions
        for (func_id, function) in &object.functions {
            self.declare_function(function, context)?;
            self.function_names
                .insert(func_id.0, function.name.clone());
        }

        // Second pass: generate function bodies
        for function in object.functions.values() {
            self.generate_function(function, context)?;
        }

        // Generate main code block
        self.generate_block(&object.code, context)?;

        // Recursively handle subobjects
        for subobject in &object.subobjects {
            self.generate_object(subobject, context)?;
        }

        Ok(())
    }

    /// Declares a function (without generating body).
    fn declare_function(
        &mut self,
        function: &Function,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        let argument_types: Vec<_> = function
            .params
            .iter()
            .map(|(_, ty)| self.ir_type_to_llvm(*ty, context))
            .collect();

        let function_type = context.function_type(argument_types, function.returns.len());

        context.add_function(
            &function.name,
            function_type,
            function.returns.len(),
            Some(inkwell::module::Linkage::Internal),
            None,
            true,
        )?;

        Ok(())
    }

    /// Generates LLVM IR for a function body.
    fn generate_function(
        &mut self,
        function: &Function,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        context.set_current_function(&function.name, None, true)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        // Set up parameters
        for (index, (param_id, param_ty)) in function.params.iter().enumerate() {
            let llvm_ty = self.ir_type_to_llvm(*param_ty, context);
            let pointer = context.build_alloca(llvm_ty, &format!("param_{}", index));
            context.build_store(
                pointer,
                context.current_function().borrow().get_nth_param(index),
            )?;
            // Load the value so it's available
            let value = context.build_load(pointer, &format!("param_{}_val", index))?;
            self.set_value(*param_id, value);
        }

        // Generate body
        self.generate_block(&function.body, context)?;

        // Build return
        let return_block = context.current_function().borrow().return_block();
        context.build_unconditional_branch(return_block);
        context.set_basic_block(return_block);
        context.build_return(None);

        context.pop_debug_scope();
        Ok(())
    }

    /// Generates LLVM IR for a block.
    fn generate_block(&mut self, block: &Block, context: &mut PolkaVMContext<'ctx>) -> Result<()> {
        for stmt in &block.statements {
            self.generate_statement(stmt, context)?;
        }
        Ok(())
    }

    /// Generates LLVM IR for a region.
    fn generate_region(
        &mut self,
        region: &Region,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        for stmt in &region.statements {
            self.generate_statement(stmt, context)?;
        }
        Ok(())
    }

    /// Generates LLVM IR for a statement.
    fn generate_statement(
        &mut self,
        stmt: &Statement,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        match stmt {
            Statement::Let { bindings, value } => {
                let llvm_value = self.generate_expr(value, context)?;
                // For single binding
                if bindings.len() == 1 {
                    self.set_value(bindings[0], llvm_value);
                } else {
                    // Tuple unpacking - extract each element
                    let struct_val = llvm_value.into_struct_value();
                    for (index, binding) in bindings.iter().enumerate() {
                        let field = context
                            .builder()
                            .build_extract_value(
                                struct_val,
                                index as u32,
                                &format!("field_{}", index),
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        self.set_value(*binding, field);
                    }
                }
            }

            Statement::MStore {
                offset,
                value,
                region: _,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let value_val = self.translate_value(value)?.into_int_value();
                revive_llvm_context::polkavm_evm_memory::store(context, offset_val, value_val)?;
            }

            Statement::MStore8 {
                offset,
                value,
                region: _,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let value_val = self.translate_value(value)?.into_int_value();
                revive_llvm_context::polkavm_evm_memory::store_byte(context, offset_val, value_val)?;
            }

            Statement::MCopy { dest, src, length } => {
                // MCopy is handled via memory operations
                let _dest_val = self.translate_value(dest)?.into_int_value();
                let _src_val = self.translate_value(src)?.into_int_value();
                let _len_val = self.translate_value(length)?.into_int_value();
                // TODO: Implement mcopy using a loop or memcpy intrinsic
            }

            Statement::SStore {
                key,
                value,
                static_slot: _,
            } => {
                let key_arg = self.value_to_argument(key)?;
                let value_arg = self.value_to_argument(value)?;
                revive_llvm_context::polkavm_evm_storage::store(context, &key_arg, &value_arg)?;
            }

            Statement::TStore { key, value } => {
                let key_arg = self.value_to_argument(key)?;
                let value_arg = self.value_to_argument(value)?;
                revive_llvm_context::polkavm_evm_storage::transient_store(
                    context, &key_arg, &value_arg,
                )?;
            }

            Statement::If {
                condition,
                inputs: _,
                then_region,
                else_region,
                outputs: _,
            } => {
                let cond_val = self.translate_value(condition)?.into_int_value();
                // Convert to i1 (compare != 0)
                let cond_bool = context
                    .builder()
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        cond_val,
                        context.word_type().const_zero(),
                        "cond_bool",
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                let then_block = context.append_basic_block("if_then");
                let join_block = context.append_basic_block("if_join");

                if let Some(else_region) = else_region {
                    let else_block = context.append_basic_block("if_else");
                    context.build_conditional_branch(cond_bool, then_block, else_block)?;

                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    context.build_unconditional_branch(join_block);

                    context.set_basic_block(else_block);
                    self.generate_region(else_region, context)?;
                    context.build_unconditional_branch(join_block);
                } else {
                    context.build_conditional_branch(cond_bool, then_block, join_block)?;

                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    context.build_unconditional_branch(join_block);
                }

                context.set_basic_block(join_block);
            }

            Statement::Switch {
                scrutinee,
                inputs: _,
                cases,
                default,
                outputs: _,
            } => {
                let scrut_val = self.translate_value(scrutinee)?.into_int_value();
                let join_block = context.append_basic_block("switch_join");

                // Create case blocks
                let mut case_blocks = Vec::new();
                for (idx, case) in cases.iter().enumerate() {
                    let case_block = context.append_basic_block(&format!("switch_case_{}", idx));
                    let case_val = context.word_const(
                        case.value
                            .to_u64()
                            .unwrap_or_else(|| panic!("Case value too large")),
                    );
                    case_blocks.push((case_val, case_block, &case.body));
                }

                // Create default block
                let default_block = context.append_basic_block("switch_default");

                // Build switch instruction
                let switch_cases: Vec<_> = case_blocks
                    .iter()
                    .map(|(val, block, _)| (*val, *block))
                    .collect();
                context
                    .builder()
                    .build_switch(scrut_val, default_block, &switch_cases)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                // Generate case bodies
                for (_, case_block, body) in case_blocks {
                    context.set_basic_block(case_block);
                    self.generate_region(body, context)?;
                    context.build_unconditional_branch(join_block);
                }

                // Generate default
                context.set_basic_block(default_block);
                if let Some(default_region) = default {
                    self.generate_region(default_region, context)?;
                }
                context.build_unconditional_branch(join_block);

                context.set_basic_block(join_block);
            }

            Statement::For {
                init_values,
                loop_vars,
                condition,
                body,
                post,
                outputs: _,
            } => {
                // Initialize loop variables
                for (init_val, loop_var) in init_values.iter().zip(loop_vars.iter()) {
                    let val = self.translate_value(init_val)?;
                    self.set_value(*loop_var, val);
                }

                let cond_block = context.append_basic_block("for_cond");
                let body_block = context.append_basic_block("for_body");
                let post_block = context.append_basic_block("for_post");
                let join_block = context.append_basic_block("for_join");

                context.build_unconditional_branch(cond_block);
                context.set_basic_block(cond_block);

                // Evaluate condition
                let cond_val = self.generate_expr(condition, context)?;
                let cond_bool = context
                    .builder()
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        cond_val.into_int_value(),
                        context.word_type().const_zero(),
                        "for_cond_bool",
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                context.build_conditional_branch(cond_bool, body_block, join_block)?;

                // Push loop for break/continue
                context.push_loop(body_block, post_block, join_block);

                context.set_basic_block(body_block);
                self.generate_region(body, context)?;
                context.build_unconditional_branch(post_block);

                context.set_basic_block(post_block);
                self.generate_region(post, context)?;
                context.build_unconditional_branch(cond_block);

                context.pop_loop();
                context.set_basic_block(join_block);
            }

            Statement::Break => {
                let join_block = context.r#loop().join_block;
                context.build_unconditional_branch(join_block);
                // Create unreachable block for dead code after break
                let unreachable = context.append_basic_block("break_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Continue => {
                let continue_block = context.r#loop().continue_block;
                context.build_unconditional_branch(continue_block);
                let unreachable = context.append_basic_block("continue_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Leave => {
                let return_block = context.current_function().borrow().return_block();
                context.build_unconditional_branch(return_block);
                let unreachable = context.append_basic_block("leave_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Revert { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                revive_llvm_context::polkavm_evm_return::revert(context, offset_val, length_val)?;
            }

            Statement::Return { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                revive_llvm_context::polkavm_evm_return::r#return(
                    context, offset_val, length_val,
                )?;
            }

            Statement::Stop => {
                revive_llvm_context::polkavm_evm_return::stop(context)?;
            }

            Statement::Invalid => {
                revive_llvm_context::polkavm_evm_return::invalid(context)?;
            }

            Statement::SelfDestruct { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                revive_llvm_context::polkavm_evm_return::selfdestruct(context, addr_val)?;
            }

            Statement::ExternalCall { result, .. } => {
                // TODO: Implement external call handling
                // For now, set result to 0
                let zero = context.word_const(0).as_basic_value_enum();
                self.set_value(*result, zero);
            }

            Statement::Create { result, .. } => {
                // TODO: Implement create handling
                // For now, set result to 0
                let zero = context.word_const(0).as_basic_value_enum();
                self.set_value(*result, zero);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                let topic_vals: Vec<BasicValueEnum<'ctx>> = topics
                    .iter()
                    .map(|t| self.translate_value(t))
                    .collect::<Result<_>>()?;

                match topic_vals.len() {
                    0 => revive_llvm_context::polkavm_evm_event::log::<0>(
                        context,
                        offset_val,
                        length_val,
                        [],
                    )?,
                    1 => revive_llvm_context::polkavm_evm_event::log::<1>(
                        context,
                        offset_val,
                        length_val,
                        [topic_vals[0]],
                    )?,
                    2 => revive_llvm_context::polkavm_evm_event::log::<2>(
                        context,
                        offset_val,
                        length_val,
                        [topic_vals[0], topic_vals[1]],
                    )?,
                    3 => revive_llvm_context::polkavm_evm_event::log::<3>(
                        context,
                        offset_val,
                        length_val,
                        [topic_vals[0], topic_vals[1], topic_vals[2]],
                    )?,
                    4 => revive_llvm_context::polkavm_evm_event::log::<4>(
                        context,
                        offset_val,
                        length_val,
                        [topic_vals[0], topic_vals[1], topic_vals[2], topic_vals[3]],
                    )?,
                    _ => return Err(CodegenError::Unsupported("log with >4 topics".into())),
                }
            }

            Statement::CodeCopy { .. }
            | Statement::ExtCodeCopy { .. }
            | Statement::ReturnDataCopy { .. }
            | Statement::DataCopy { .. }
            | Statement::CallDataCopy { .. } => {
                // TODO: Implement these copy operations
            }

            Statement::Block(region) => {
                self.generate_region(region, context)?;
            }

            Statement::Expr(expr) => {
                // Evaluate for side effects, discard result
                let _ = self.generate_expr(expr, context)?;
            }
        }
        Ok(())
    }

    /// Generates LLVM IR for an expression.
    fn generate_expr(
        &mut self,
        expr: &Expr,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Literal { value, ty: _ } => {
                // Convert BigUint to u64 for LLVM constant
                let val = value.to_u64().unwrap_or(0);
                Ok(context.word_const(val).as_basic_value_enum())
            }

            Expr::Var(id) => self.get_value(*id),

            Expr::Binary { op, lhs, rhs } => {
                let lhs_val = self.translate_value(lhs)?.into_int_value();
                let rhs_val = self.translate_value(rhs)?.into_int_value();
                self.generate_binop(*op, lhs_val, rhs_val, context)
            }

            Expr::Ternary { op, a, b, n } => {
                let a_val = self.translate_value(a)?.into_int_value();
                let b_val = self.translate_value(b)?.into_int_value();
                let n_val = self.translate_value(n)?.into_int_value();

                match op {
                    BinOp::AddMod => Ok(revive_llvm_context::polkavm_evm_math::add_mod(
                        context, a_val, b_val, n_val,
                    )?),
                    BinOp::MulMod => Ok(revive_llvm_context::polkavm_evm_math::mul_mod(
                        context, a_val, b_val, n_val,
                    )?),
                    _ => Err(CodegenError::Unsupported(format!(
                        "Ternary operation {:?}",
                        op
                    ))),
                }
            }

            Expr::Unary { op, operand } => {
                let operand_val = self.translate_value(operand)?.into_int_value();
                match op {
                    UnaryOp::IsZero => {
                        // IsZero: result is 1 if operand == 0, else 0
                        let is_zero = context
                            .builder()
                            .build_int_compare(
                                inkwell::IntPredicate::EQ,
                                operand_val,
                                context.word_type().const_zero(),
                                "iszero",
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        let result = context
                            .builder()
                            .build_int_z_extend(is_zero, context.word_type(), "iszero_ext")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        Ok(result.as_basic_value_enum())
                    }
                    UnaryOp::Not => {
                        // Bitwise NOT
                        let result = context
                            .builder()
                            .build_not(operand_val, "not")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        Ok(result.as_basic_value_enum())
                    }
                }
            }

            Expr::CallDataLoad { offset } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_calldata::load(context, offset_val)?)
            }

            Expr::CallValue => Ok(revive_llvm_context::polkavm_evm_ether_gas::value(context)?),

            Expr::Caller => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::caller(context)?)
            }

            Expr::Origin => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::origin(context)?)
            }

            Expr::CallDataSize => Ok(revive_llvm_context::polkavm_evm_calldata::size(context)?),

            Expr::CodeSize => Ok(revive_llvm_context::polkavm_evm_ext_code::size(context, None)?),

            Expr::GasPrice => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_price(context)?)
            }

            Expr::ExtCodeSize { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_ext_code::size(context, Some(addr_val))?)
            }

            Expr::ReturnDataSize => {
                Ok(revive_llvm_context::polkavm_evm_return_data::size(context)?)
            }

            Expr::ExtCodeHash { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_ext_code::hash(context, addr_val)?)
            }

            Expr::BlockHash { number } => {
                let num_val = self.translate_value(number)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_contract_context::block_hash(context, num_val)?)
            }

            Expr::Coinbase => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::coinbase(context)?)
            }

            Expr::Timestamp => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::block_timestamp(context)?)
            }

            Expr::Number => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::block_number(context)?)
            }

            Expr::Difficulty => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::difficulty(context)?)
            }

            Expr::GasLimit => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_limit(context)?)
            }

            Expr::ChainId => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::chain_id(context)?)
            }

            Expr::SelfBalance => {
                Ok(revive_llvm_context::polkavm_evm_ether_gas::self_balance(context)?)
            }

            Expr::BaseFee => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::basefee(context)?)
            }

            Expr::BlobHash { .. } | Expr::BlobBaseFee => {
                // Blob opcodes return 0 for now (EIP-4844)
                Ok(context.word_const(0).as_basic_value_enum())
            }

            Expr::Gas => Ok(revive_llvm_context::polkavm_evm_ether_gas::gas(context)?),

            Expr::MSize => Ok(revive_llvm_context::polkavm_evm_memory::msize(context)?),

            Expr::Address => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::address(context)?)
            }

            Expr::Balance { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_ether_gas::balance(context, addr_val)?)
            }

            Expr::MLoad { offset, region: _ } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_memory::load(context, offset_val)?)
            }

            Expr::SLoad { key, static_slot: _ } => {
                let key_arg = self.value_to_argument(key)?;
                Ok(revive_llvm_context::polkavm_evm_storage::load(context, &key_arg)?)
            }

            Expr::TLoad { key } => {
                let key_arg = self.value_to_argument(key)?;
                Ok(revive_llvm_context::polkavm_evm_storage::transient_load(context, &key_arg)?)
            }

            Expr::Call { function, args } => {
                let func_name = self
                    .function_names
                    .get(&function.0)
                    .ok_or(CodegenError::UndefinedFunction(*function))?
                    .clone();

                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.translate_value(arg)?);
                }

                let func = context
                    .get_function(&func_name, true)
                    .ok_or_else(|| CodegenError::UndefinedFunction(*function))?;
                let result = context.build_call(
                    func.borrow().declaration(),
                    &arg_vals,
                    &format!("{}_result", func_name),
                );

                result.ok_or_else(|| CodegenError::Llvm("Function call failed".into()))
            }

            Expr::Truncate { value, to } => {
                let val = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_truncate(val, target_type, "truncate")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expr::ZeroExtend { value, to } => {
                let val = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_z_extend(val, target_type, "zext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expr::SignExtendTo { value, to } => {
                let val = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_s_extend(val, target_type, "sext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expr::Keccak256 { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3(
                    context,
                    offset_val,
                    length_val,
                )?)
            }

            Expr::DataOffset { .. } | Expr::DataSize { .. } => {
                // TODO: Implement dataoffset/datasize
                Ok(context.word_const(0).as_basic_value_enum())
            }
        }
    }

    /// Generates a binary operation.
    fn generate_binop(
        &mut self,
        op: BinOp,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match op {
            BinOp::Add => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::addition(context, lhs, rhs)?)
            }
            BinOp::Sub => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::subtraction(context, lhs, rhs)?)
            }
            BinOp::Mul => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::multiplication(context, lhs, rhs)?)
            }
            BinOp::Div => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::division(context, lhs, rhs)?)
            }
            BinOp::SDiv => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::division_signed(context, lhs, rhs)?)
            }
            BinOp::Mod => {
                Ok(revive_llvm_context::polkavm_evm_arithmetic::remainder(context, lhs, rhs)?)
            }
            BinOp::SMod => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::remainder_signed(context, lhs, rhs)?,
            ),
            BinOp::Exp => {
                Ok(revive_llvm_context::polkavm_evm_math::exponent(context, lhs, rhs)?)
            }
            BinOp::And => Ok(revive_llvm_context::polkavm_evm_bitwise::and(context, lhs, rhs)?),
            BinOp::Or => Ok(revive_llvm_context::polkavm_evm_bitwise::or(context, lhs, rhs)?),
            BinOp::Xor => Ok(revive_llvm_context::polkavm_evm_bitwise::xor(context, lhs, rhs)?),
            BinOp::Shl => {
                Ok(revive_llvm_context::polkavm_evm_bitwise::shift_left(context, lhs, rhs)?)
            }
            BinOp::Shr => {
                Ok(revive_llvm_context::polkavm_evm_bitwise::shift_right(context, lhs, rhs)?)
            }
            BinOp::Sar => Ok(
                revive_llvm_context::polkavm_evm_bitwise::shift_right_arithmetic(context, lhs, rhs)?,
            ),
            BinOp::Lt => Ok(revive_llvm_context::polkavm_evm_comparison::compare(
                context,
                lhs,
                rhs,
                inkwell::IntPredicate::ULT,
            )?),
            BinOp::Gt => Ok(revive_llvm_context::polkavm_evm_comparison::compare(
                context,
                lhs,
                rhs,
                inkwell::IntPredicate::UGT,
            )?),
            BinOp::Slt => Ok(revive_llvm_context::polkavm_evm_comparison::compare(
                context,
                lhs,
                rhs,
                inkwell::IntPredicate::SLT,
            )?),
            BinOp::Sgt => Ok(revive_llvm_context::polkavm_evm_comparison::compare(
                context,
                lhs,
                rhs,
                inkwell::IntPredicate::SGT,
            )?),
            BinOp::Eq => {
                // Equal comparison
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::EQ, lhs, rhs, "eq")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                let result = context
                    .builder()
                    .build_int_z_extend(cmp, context.word_type(), "eq_ext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(result.as_basic_value_enum())
            }
            BinOp::Byte => {
                Ok(revive_llvm_context::polkavm_evm_bitwise::byte(context, lhs, rhs)?)
            }
            BinOp::SignExtend => {
                Ok(revive_llvm_context::polkavm_evm_math::sign_extend(context, lhs, rhs)?)
            }
            BinOp::AddMod | BinOp::MulMod => {
                // These are ternary ops, shouldn't reach here
                Err(CodegenError::Unsupported(format!(
                    "Binary call for ternary op {:?}",
                    op
                )))
            }
        }
    }

    /// Converts an IR type to LLVM type.
    fn ir_type_to_llvm(
        &self,
        ty: Type,
        context: &PolkaVMContext<'ctx>,
    ) -> inkwell::types::BasicTypeEnum<'ctx> {
        match ty {
            Type::Int(width) => context
                .integer_type(width.bits() as usize)
                .as_basic_type_enum(),
            Type::Ptr(_) => context.word_type().as_basic_type_enum(), // Pointers are word-sized
            Type::Void => context.word_type().as_basic_type_enum(),   // Void defaults to word
        }
    }

    /// Converts a Value to a PolkaVMArgument for storage operations.
    fn value_to_argument(&self, value: &Value) -> Result<PolkaVMArgument<'ctx>> {
        let llvm_val = self.translate_value(value)?;
        Ok(PolkaVMArgument::value(llvm_val))
    }
}

impl Default for LlvmCodegen<'_> {
    fn default() -> Self {
        Self::new()
    }
}
