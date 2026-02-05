//! LLVM code generation for the newyork IR.
//!
//! This module implements translation from newyork IR to LLVM IR via inkwell,
//! reusing the PolkaVM context infrastructure from revive-llvm-context.
//!
//! NOTE: This is a work-in-progress implementation. Many functions are stubbed
//! or simplified for initial development.

use std::collections::{BTreeMap, BTreeSet};

use inkwell::types::BasicType;
use inkwell::values::{AnyValue, BasicValue, BasicValueEnum, IntValue};
use num::ToPrimitive;
use revive_llvm_context::{
    PolkaVMArgument, PolkaVMContext, PolkaVMFunctionDeployCode, PolkaVMFunctionRuntimeCode,
};

use crate::heap_opt::HeapOptResults;
use crate::ir::{
    BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId, Object, Region,
    Statement, Type, UnaryOp, Value, ValueId,
};
use crate::type_inference::TypeInference;

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

    #[error("{0}")]
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
    /// Set of function names that have already been generated.
    /// This is used to avoid regenerating shared utility functions in multi-contract scenarios.
    generated_functions: BTreeSet<String>,
    /// Heap optimization results for skipping byte-swapping on internal memory.
    heap_opt: HeapOptResults,
    /// Type inference results for using narrower types.
    type_info: TypeInference,
}

impl<'ctx> LlvmCodegen<'ctx> {
    /// Creates a new code generator with optimization results.
    pub fn new(heap_opt: HeapOptResults, type_info: TypeInference) -> Self {
        LlvmCodegen {
            values: BTreeMap::new(),
            function_names: BTreeMap::new(),
            generated_functions: BTreeSet::new(),
            heap_opt,
            type_info,
        }
    }

    /// Creates a new code generator that shares the generated_functions set with another.
    /// This is used for subobjects to avoid regenerating shared utility functions.
    pub fn new_with_shared_functions(
        generated_functions: BTreeSet<String>,
        heap_opt: HeapOptResults,
        type_info: TypeInference,
    ) -> Self {
        LlvmCodegen {
            values: BTreeMap::new(),
            function_names: BTreeMap::new(),
            generated_functions,
            heap_opt,
            type_info,
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
        self.values.get(&value.id.0).copied().ok_or_else(|| {
            let mut defined_ids: Vec<_> = self.values.keys().copied().collect();
            defined_ids.sort();
            let max_id = defined_ids.iter().max().copied().unwrap_or(0);
            // Find the nearest defined IDs
            let lower_bound = defined_ids
                .iter()
                .filter(|&&x| x < value.id.0)
                .max()
                .copied();
            let upper_bound = defined_ids
                .iter()
                .filter(|&&x| x > value.id.0)
                .min()
                .copied();
            CodegenError::Llvm(format!(
                "Undefined value {:?} (nearest: {:?}..{:?}, max: {}, total: {})",
                value.id,
                lower_bound,
                upper_bound,
                max_id,
                defined_ids.len()
            ))
        })
    }

    /// Tries to extract a constant u64 offset from an LLVM IntValue.
    /// Returns Some(offset) if the value is a constant that fits in u64.
    /// Used for per-access native memory optimization.
    fn try_extract_const_offset(&self, value: IntValue<'ctx>) -> Option<u64> {
        // For 256-bit integers, get_zero_extended_constant returns None
        // because the value doesn't fit in u64. We need to check if it's
        // a small value that fits in u64 by looking at the actual constant.
        if !value.is_const() {
            return None;
        }

        // Try to get as zero-extended constant (works for <= 64-bit types)
        if let Some(v) = value.get_zero_extended_constant() {
            return Some(v);
        }

        // For i256 constants, we need to check if the high bits are zero
        // by converting to string and parsing
        let s = value.print_to_string().to_string();
        // Format is "i256 <value>" - extract the value
        if let Some(val_str) = s.strip_prefix("i256 ") {
            if let Ok(v) = val_str.trim().parse::<u64>() {
                return Some(v);
            }
        }

        None
    }

    /// Checks if a memory operation at the given offset can use native byte order.
    /// Returns true in two cases:
    /// 1. all_native mode: ALL heap accesses are safe, so we use native functions exclusively
    /// 2. Per-access mode: This specific offset is known to be safe, use inline native code
    ///
    /// Native memory (no byte-swapping) requires:
    /// - Statically known offset (no dynamic/unknown accesses)
    /// - Word-aligned (offset is multiple of 32)
    /// - Not escaping to external code (no calls, returns, logs)
    /// - Not tainted by unaligned writes
    ///
    /// When all_native() is true, we use native functions (saves ~200 bytes).
    /// When per-access native is possible, we use inline code (no function overhead).
    fn can_use_native_memory(&self, offset: IntValue<'ctx>) -> bool {
        // If ALL accesses are safe, use native mode
        if self.heap_opt.all_native() {
            return true;
        }

        // Try per-access check: if this specific offset is known-safe, we can inline native code
        if let Some(const_offset) = self.try_extract_const_offset(offset) {
            if self.heap_opt.can_use_native(const_offset) {
                return true;
            }
        }

        // Otherwise, must use byte-swapping
        false
    }

    /// Returns true if ALL memory accesses can use native byte order.
    /// Used to decide whether to emit only native heap functions.
    fn is_all_native(&self) -> bool {
        self.heap_opt.all_native()
    }

    /// Truncates a 256-bit offset to 32-bit for use with native memory operations.
    /// The native load/store functions and inline native operations expect xlen_type (32-bit) offsets.
    fn truncate_offset_to_xlen(
        &self,
        context: &PolkaVMContext<'ctx>,
        offset: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let xlen_type = context.xlen_type();
        if offset.get_type().get_bit_width() == xlen_type.get_bit_width() {
            Ok(offset)
        } else {
            context
                .builder()
                .build_int_truncate(offset, xlen_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        }
    }

    /// Gets the inferred bit-width for a value.
    /// Phase 2 infrastructure: available for future optimizations that query type inference.
    #[allow(dead_code)]
    fn inferred_width(&self, id: ValueId) -> BitWidth {
        self.type_info.get(id).min_width
    }

    /// Returns true if a value can use a narrow type (64-bit or less).
    /// This is safe when the inferred width fits and the value is only used
    /// in contexts that support narrow types (comparisons, memory offsets).
    /// Phase 2 infrastructure: available for future optimizations.
    #[allow(dead_code)]
    fn can_use_narrow_type(&self, id: ValueId) -> bool {
        let width = self.inferred_width(id);
        matches!(
            width,
            BitWidth::I1 | BitWidth::I8 | BitWidth::I32 | BitWidth::I64
        )
    }

    /// Gets the LLVM int type for a given bit-width.
    /// Phase 2 infrastructure: available for future optimizations.
    #[allow(dead_code)]
    fn int_type_for_width(
        &self,
        context: &PolkaVMContext<'ctx>,
        width: BitWidth,
    ) -> inkwell::types::IntType<'ctx> {
        match width {
            BitWidth::I1 => context.llvm().bool_type(),
            BitWidth::I8 => context.llvm().i8_type(),
            BitWidth::I32 => context.llvm().i32_type(),
            BitWidth::I64 => context.llvm().i64_type(),
            BitWidth::I160 => context.llvm().custom_width_int_type(160),
            BitWidth::I256 => context.word_type(),
        }
    }

    /// Ensures a value is extended to 256-bit word type.
    /// Used when a narrower value needs to be used in operations requiring full width.
    fn ensure_word_type(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let value_width = value.get_type().get_bit_width();
        let word_width = context.word_type().get_bit_width();

        if value_width == word_width {
            Ok(value)
        } else if value_width < word_width {
            // Zero-extend to word type
            context
                .builder()
                .build_int_z_extend(value, context.word_type(), name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        } else {
            // Shouldn't happen - truncate as fallback
            context
                .builder()
                .build_int_truncate(value, context.word_type(), name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        }
    }

    /// Ensures two values have the same type by extending the narrower one.
    /// Returns both values at the wider type.
    fn ensure_same_type(
        &self,
        context: &PolkaVMContext<'ctx>,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
        name: &str,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let a_width = a.get_type().get_bit_width();
        let b_width = b.get_type().get_bit_width();

        if a_width == b_width {
            Ok((a, b))
        } else if a_width > b_width {
            // Extend b to match a
            let b_ext = context
                .builder()
                .build_int_z_extend(b, a.get_type(), &format!("{}_ext_b", name))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
            Ok((a, b_ext))
        } else {
            // Extend a to match b
            let a_ext = context
                .builder()
                .build_int_z_extend(a, b.get_type(), &format!("{}_ext_a", name))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
            Ok((a_ext, b))
        }
    }

    /// Converts a value to the inferred type for storage.
    /// If the inferred type is narrower, truncates; if wider, zero-extends.
    /// Phase 2 infrastructure: available for future optimizations.
    #[allow(dead_code)]
    fn convert_to_inferred_type(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        target_id: ValueId,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let inferred_width = self.inferred_width(target_id);
        let target_type = self.int_type_for_width(context, inferred_width);
        let value_width = value.get_type().get_bit_width();
        let target_width = target_type.get_bit_width();

        if value_width == target_width {
            Ok(value)
        } else if value_width > target_width {
            // Truncate to narrower type
            context
                .builder()
                .build_int_truncate(value, target_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        } else {
            // Zero-extend to wider type
            context
                .builder()
                .build_int_z_extend(value, target_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        }
    }

    /// Checks if a basic block is unreachable (has no predecessors or already has a terminator).
    /// This is used to determine if a region ended early due to Leave/Break/Return/etc.
    fn block_is_unreachable(block: inkwell::basic_block::BasicBlock<'ctx>) -> bool {
        // If block has a terminator, it was already terminated
        if block.get_terminator().is_some() {
            return true;
        }
        // If block has no instructions at all and its name contains "unreachable",
        // it was created as an unreachable landing pad after Leave/Break/Continue
        if block.get_first_instruction().is_none() {
            if let Ok(name) = block.get_name().to_str() {
                if name.contains("unreachable") {
                    return true;
                }
            }
        }
        false
    }

    /// Generates LLVM IR for a complete object.
    pub fn generate_object(
        &mut self,
        object: &Object,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        // Determine if this is deploy or runtime code and set the code type
        let is_runtime = object.name.ends_with("_deployed");
        if is_runtime {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Runtime);
        } else {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Deploy);
        }

        // First pass: declare all user-defined functions
        for (func_id, function) in &object.functions {
            self.declare_function(function, context)?;
            self.function_names.insert(func_id.0, function.name.clone());
        }

        // Second pass: generate function bodies
        for function in object.functions.values() {
            self.generate_function(function, context)?;
        }

        // Generate main code block within the appropriate function context
        let function_name = if is_runtime {
            PolkaVMFunctionRuntimeCode
        } else {
            PolkaVMFunctionDeployCode
        };

        // Set the current function and basic block
        context
            .set_current_function(function_name, None, false)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        // Generate the main code block
        self.generate_block(&object.code, context)?;

        // Reset debug location and handle function return
        context
            .set_debug_location(0, 0, None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Check if block ends with a terminator, if not add branch to return
        match context
            .basic_block()
            .get_last_instruction()
            .map(|i| i.get_opcode())
        {
            Some(inkwell::values::InstructionOpcode::Br) => {}
            Some(inkwell::values::InstructionOpcode::Switch) => {}
            Some(inkwell::values::InstructionOpcode::Return) => {}
            Some(inkwell::values::InstructionOpcode::Unreachable) => {}
            _ => {
                context
                    .build_unconditional_branch(context.current_function().borrow().return_block());
            }
        }

        // Build the return block
        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_return(None);

        context.pop_debug_scope();

        // Recursively handle subobjects (inner_object for deployed code)
        // Each subobject gets a fresh codegen instance for values (SSA values are scoped to objects)
        // but shares the generated_functions set to avoid regenerating shared utility functions
        for subobject in &object.subobjects {
            let mut subobject_codegen = LlvmCodegen::new_with_shared_functions(
                self.generated_functions.clone(),
                self.heap_opt.clone(),
                self.type_info.clone(),
            );
            subobject_codegen.generate_object(subobject, context)?;
            // Merge back any new generated functions
            self.generated_functions
                .extend(subobject_codegen.generated_functions);
        }

        Ok(())
    }

    /// Declares a function (without generating body).
    /// If the function already exists (e.g., shared utility functions in multi-contract scenarios),
    /// this will skip re-declaring it.
    pub fn declare_function(
        &mut self,
        function: &Function,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        // Check if function already exists (handles shared utility functions)
        if context.get_function(&function.name, true).is_some() {
            return Ok(());
        }

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
    /// If the function body has already been generated (shared utility functions in multi-contract
    /// scenarios), this will skip regenerating it.
    fn generate_function(
        &mut self,
        function: &Function,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        // Build the internal function name (includes _Deploy or _Runtime suffix)
        let internal_name = format!(
            "{}_{}",
            function.name,
            context
                .code_type()
                .map(|c| format!("{:?}", c))
                .unwrap_or_default()
        );

        // Skip if this function's body has already been generated
        // We use the internal name (with code_type suffix) to properly track
        // deploy vs runtime variants of the same function
        if self.generated_functions.contains(&internal_name) {
            return Ok(());
        }
        self.generated_functions.insert(internal_name);

        // Save the current values map and start fresh for this function
        // Each function has its own SSA namespace
        let saved_values = std::mem::take(&mut self.values);

        context.set_current_function(&function.name, None, true)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        // Set up parameters - use the parameter values directly
        for (index, (param_id, _param_ty)) in function.params.iter().enumerate() {
            let param_value = context.current_function().borrow().get_nth_param(index);
            self.set_value(*param_id, param_value);
        }

        // Initialize return values to zero
        // Return variables in Yul start at zero and can be assigned in the body.
        // We need to initialize BOTH the initial IDs (used by If statements as "before" values)
        // AND the final IDs (used for the actual return).
        let zero = context.word_const(0).as_basic_value_enum();
        for ret_id in &function.return_values_initial {
            self.set_value(*ret_id, zero);
        }
        // Also set the final return value IDs (they may be the same as initial or different)
        for ret_id in &function.return_values {
            self.set_value(*ret_id, zero);
        }

        // Generate body
        self.generate_block(&function.body, context)?;

        // Store return values to return pointer before going to return block
        match context.current_function().borrow().r#return() {
            revive_llvm_context::PolkaVMFunctionReturn::None => {}
            revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                // Single return value - must be word type for the pointer
                if !function.return_values.is_empty() {
                    if let Ok(ret_val) = self.get_value(function.return_values[0]) {
                        let ret_val = if ret_val.is_int_value() {
                            self.ensure_word_type(context, ret_val.into_int_value(), "ret_val")?
                                .as_basic_value_enum()
                        } else {
                            ret_val
                        };
                        context.build_store(pointer, ret_val)?;
                    }
                }
            }
            revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                // Multiple return values - build a struct
                // Struct fields are word type, so ensure each value is word type
                let field_types: Vec<_> = (0..size)
                    .map(|_| context.word_type().as_basic_type_enum())
                    .collect();
                let struct_type = context.structure_type(&field_types);
                let mut struct_val = struct_type.get_undef();
                for (i, ret_id) in function.return_values.iter().enumerate() {
                    if let Ok(ret_val) = self.get_value(*ret_id) {
                        let ret_val = if ret_val.is_int_value() {
                            self.ensure_word_type(
                                context,
                                ret_val.into_int_value(),
                                &format!("ret_val_{}", i),
                            )?
                            .as_basic_value_enum()
                        } else {
                            ret_val
                        };
                        struct_val = context
                            .builder()
                            .build_insert_value(struct_val, ret_val, i as u32, "ret_insert")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                            .into_struct_value();
                    }
                }
                context.build_store(pointer, struct_val.as_basic_value_enum())?;
            }
        }

        // Build return - handle based on return type
        let return_block = context.current_function().borrow().return_block();
        context.build_unconditional_branch(return_block);
        context.set_basic_block(return_block);

        // Get the return type and build appropriate return
        match context.current_function().borrow().r#return() {
            revive_llvm_context::PolkaVMFunctionReturn::None => {
                context.build_return(None);
            }
            revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                let return_value = context
                    .build_load(pointer, "return_value")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                context.build_return(Some(&return_value));
            }
            revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, .. } => {
                let return_value = context
                    .build_load(pointer, "return_value")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                context.build_return(Some(&return_value));
            }
        }

        context.pop_debug_scope();

        // Restore the saved values map from before this function
        self.values = saved_values;

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
        // Track which statement we're processing for better error messages
        let stmt_kind = match stmt {
            Statement::Let { .. } => "Let",
            Statement::MStore { .. } => "MStore",
            Statement::MStore8 { .. } => "MStore8",
            Statement::MCopy { .. } => "MCopy",
            Statement::SStore { .. } => "SStore",
            Statement::TStore { .. } => "TStore",
            Statement::If { .. } => "If",
            Statement::Switch { .. } => "Switch",
            Statement::For { .. } => "For",
            Statement::Return { .. } => "Return",
            Statement::Revert { .. } => "Revert",
            Statement::Leave { .. } => "Leave",
            Statement::ExternalCall { .. } => "ExternalCall",
            Statement::Create { .. } => "Create",
            Statement::SelfDestruct { .. } => "SelfDestruct",
            Statement::Log { .. } => "Log",
            Statement::CodeCopy { .. } => "CodeCopy",
            Statement::ExtCodeCopy { .. } => "ExtCodeCopy",
            Statement::ReturnDataCopy { .. } => "ReturnDataCopy",
            Statement::CallDataCopy { .. } => "CallDataCopy",
            Statement::Block(_) => "Block",
            Statement::Expr(_) => "Expr",
            Statement::SetImmutable { .. } => "SetImmutable",
            Statement::Continue => "Continue",
            Statement::Break => "Break",
            Statement::Stop => "Stop",
            Statement::Invalid => "Invalid",
            Statement::DataCopy { .. } => "DataCopy",
        };

        if let Err(e) = self.generate_statement_inner(stmt, context) {
            return Err(CodegenError::Llvm(format!(
                "Error in {} statement: {}",
                stmt_kind, e
            )));
        }
        Ok(())
    }

    /// Inner implementation of generate_statement.
    fn generate_statement_inner(
        &mut self,
        stmt: &Statement,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        match stmt {
            Statement::Let { bindings, value } => {
                let llvm_value = self.generate_expr(value, context)?;
                // For single binding
                if bindings.len() == 1 {
                    let binding_id = bindings[0];
                    // Store value at word type for compatibility with runtime functions.
                    // Type inference infrastructure is in place for future use, but
                    // currently all values are kept at word type.
                    self.set_value(binding_id, llvm_value);
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
                        // Store tuple element at word type
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
                // MStore requires 256-bit value
                let value_val = self.ensure_word_type(context, value_val, "mstore_val")?;
                if self.can_use_native_memory(offset_val) {
                    // Native memory operations require 32-bit offset (xlen_type)
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "mstore_offset_xlen")?;
                    if self.is_all_native() {
                        // All accesses native: use runtime function (function body is emitted)
                        revive_llvm_context::polkavm_evm_memory::store_native(
                            context,
                            offset_xlen,
                            value_val,
                        )?;
                    } else {
                        // Per-access native: inline the store (no function overhead)
                        revive_llvm_context::polkavm_evm_memory::store_native_inline(
                            context,
                            offset_xlen,
                            value_val,
                        )?;
                    }
                } else {
                    revive_llvm_context::polkavm_evm_memory::store(context, offset_val, value_val)?;
                }
            }

            Statement::MStore8 {
                offset,
                value,
                region: _,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let value_val = self.translate_value(value)?.into_int_value();
                // MStore8 expects word-sized value (takes low byte)
                let value_val = self.ensure_word_type(context, value_val, "mstore8_val")?;
                revive_llvm_context::polkavm_evm_memory::store_byte(
                    context, offset_val, value_val,
                )?;
            }

            Statement::MCopy { dest, src, length } => {
                let dest_val = self.translate_value(dest)?.into_int_value();
                let src_val = self.translate_value(src)?.into_int_value();
                let len_val = self.translate_value(length)?.into_int_value();

                let dest_pointer = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    dest_val,
                    "mcopy_destination",
                );
                let src_pointer = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    src_val,
                    "mcopy_source",
                );

                context.build_memcpy(dest_pointer, src_pointer, len_val, "mcopy_size")?;
            }

            Statement::SStore {
                key,
                value,
                static_slot: _,
            } => {
                let key_arg = self.value_to_argument(key, context)?;
                let value_arg = self.value_to_argument(value, context)?;
                revive_llvm_context::polkavm_evm_storage::store(context, &key_arg, &value_arg)?;
            }

            Statement::TStore { key, value } => {
                let key_arg = self.value_to_argument(key, context)?;
                let value_arg = self.value_to_argument(value, context)?;
                revive_llvm_context::polkavm_evm_storage::transient_store(
                    context, &key_arg, &value_arg,
                )?;
            }

            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                let cond_val = self.translate_value(condition)?.into_int_value();
                // Ensure condition is word type before comparing
                let cond_val = self.ensure_word_type(context, cond_val, "if_cond")?;
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

                // Track which branches contribute to phi nodes
                // (branches that end with terminators like Leave/Break don't contribute)
                let mut phi_incoming: Vec<(
                    Vec<BasicValueEnum<'ctx>>,
                    inkwell::basic_block::BasicBlock<'ctx>,
                )> = Vec::new();

                if let Some(else_region) = else_region {
                    let else_block = context.append_basic_block("if_else");
                    context.build_conditional_branch(cond_bool, then_block, else_block)?;

                    // Generate then branch
                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    let then_end_block = context.basic_block();
                    // Only collect yields and branch if the block is reachable
                    if !Self::block_is_unreachable(then_end_block) {
                        let mut then_yields = Vec::new();
                        for (i, yield_val) in then_region.yields.iter().enumerate() {
                            then_yields.push(self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("then_yield_{}", i),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((then_yields, then_end_block));
                    } else if then_end_block.get_terminator().is_none() {
                        // Add unreachable terminator for blocks that don't have one
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }

                    // Generate else branch
                    context.set_basic_block(else_block);
                    self.generate_region(else_region, context)?;
                    let else_end_block = context.basic_block();
                    if !Self::block_is_unreachable(else_end_block) {
                        let mut else_yields = Vec::new();
                        for (i, yield_val) in else_region.yields.iter().enumerate() {
                            else_yields.push(self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("else_yield_{}", i),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((else_yields, else_end_block));
                    } else if else_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }
                } else {
                    // No else branch - the "else" path goes directly to join
                    let entry_block = context.basic_block();
                    context.build_conditional_branch(cond_bool, then_block, join_block)?;

                    // Collect inputs as the "else" yields (from entry block to join)
                    let mut else_yields = Vec::new();
                    for (i, input_val) in inputs.iter().enumerate() {
                        else_yields.push(self.translate_value_as_word(
                            input_val,
                            context,
                            &format!("input_yield_{}", i),
                        )?);
                    }
                    phi_incoming.push((else_yields, entry_block));

                    // Generate then branch
                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    let then_end_block = context.basic_block();
                    if !Self::block_is_unreachable(then_end_block) {
                        let mut then_yields = Vec::new();
                        for (i, yield_val) in then_region.yields.iter().enumerate() {
                            then_yields.push(self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("then_yield_{}", i),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((then_yields, then_end_block));
                    } else if then_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }
                }

                context.set_basic_block(join_block);

                // Create phi nodes for outputs only if we have at least two incoming edges
                // If there's only one incoming edge, just use the value directly (no phi needed)
                if phi_incoming.len() >= 2 {
                    // Verify all yields match outputs length
                    for (yields, _) in &phi_incoming {
                        if yields.len() != outputs.len() {
                            return Err(CodegenError::Llvm(format!(
                                "If phi mismatch: {} yields vs {} outputs (outputs: {:?})",
                                yields.len(),
                                outputs.len(),
                                outputs
                            )));
                        }
                    }
                    for (i, output_id) in outputs.iter().enumerate() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("if_phi_{}", i))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        for (yields, block) in &phi_incoming {
                            phi.add_incoming(&[(&yields[i], *block)]);
                        }
                        self.set_value(*output_id, phi.as_basic_value());
                    }
                } else if phi_incoming.len() == 1 {
                    // Only one incoming edge - use the yielded values directly
                    let (yields, _) = &phi_incoming[0];
                    // Verify yields matches outputs
                    if yields.len() != outputs.len() {
                        return Err(CodegenError::Llvm(format!(
                            "If statement mismatch: {} yields vs {} outputs (outputs: {:?})",
                            yields.len(),
                            outputs.len(),
                            outputs
                        )));
                    }
                    for (i, output_id) in outputs.iter().enumerate() {
                        self.set_value(*output_id, yields[i]);
                    }
                } else {
                    // No incoming edges - the join block is unreachable (all branches terminated early)
                    // But we still need to define outputs (with undef values) in case
                    // subsequent unreachable code references them
                    for output_id in outputs.iter() {
                        self.set_value(
                            *output_id,
                            context.word_type().get_undef().as_basic_value_enum(),
                        );
                    }
                    // Add unreachable terminator
                    context
                        .builder()
                        .build_unreachable()
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    // Create a new block for any dead code that follows
                    let dead_block = context.append_basic_block("if_dead");
                    context.set_basic_block(dead_block);
                }
            }

            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                let scrut_val = self.translate_value(scrutinee)?.into_int_value();
                // Switch scrutinee must be word type to match case constants
                let scrut_val = self.ensure_word_type(context, scrut_val, "switch_scrut")?;
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

                // Collect yields from each case
                // Track which branches contribute to phi nodes
                // (branches that end with terminators like Leave/Break don't contribute)
                let mut all_yields: Vec<(
                    Vec<BasicValueEnum<'ctx>>,
                    inkwell::basic_block::BasicBlock<'ctx>,
                )> = Vec::new();

                // Generate case bodies
                for (idx, (_, case_block, body)) in case_blocks.into_iter().enumerate() {
                    context.set_basic_block(case_block);
                    self.generate_region(body, context)?;
                    let end_block = context.basic_block();

                    // Only collect yields and branch if the block is reachable
                    if !Self::block_is_unreachable(end_block) {
                        let mut yields = Vec::new();
                        for (yield_idx, yield_val) in body.yields.iter().enumerate() {
                            match self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("case_{}_yield_{}", idx, yield_idx),
                            ) {
                                Ok(v) => yields.push(v),
                                Err(e) => {
                                    return Err(CodegenError::Llvm(format!(
                                        "Switch case {} yield {}: {:?} - {}",
                                        idx, yield_idx, yield_val.id, e
                                    )));
                                }
                            }
                        }
                        context.build_unconditional_branch(join_block);
                        all_yields.push((yields, end_block));
                    } else if end_block.get_terminator().is_none() {
                        // Add unreachable terminator for blocks that don't have one
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }
                }

                // Generate default
                context.set_basic_block(default_block);
                if let Some(default_region) = default {
                    self.generate_region(default_region, context)?;
                    let default_end_block = context.basic_block();

                    if !Self::block_is_unreachable(default_end_block) {
                        let mut default_yields = Vec::new();
                        for (i, yield_val) in default_region.yields.iter().enumerate() {
                            default_yields.push(self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("default_yield_{}", i),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        all_yields.push((default_yields, default_end_block));
                    } else if default_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }
                } else {
                    // No default region - use inputs as yields
                    let default_end_block = context.basic_block();
                    let mut default_yields = Vec::new();
                    for (i, input_val) in inputs.iter().enumerate() {
                        default_yields.push(self.translate_value_as_word(
                            input_val,
                            context,
                            &format!("default_input_{}", i),
                        )?);
                    }
                    context.build_unconditional_branch(join_block);
                    all_yields.push((default_yields, default_end_block));
                }

                context.set_basic_block(join_block);

                // Create phi nodes for outputs only if we have incoming edges
                if all_yields.len() >= 2 {
                    for (i, output_id) in outputs.iter().enumerate() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("switch_phi_{}", i))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                        for (yields, end_block) in &all_yields {
                            if i < yields.len() {
                                phi.add_incoming(&[(&yields[i], *end_block)]);
                            }
                        }
                        self.set_value(*output_id, phi.as_basic_value());
                    }
                } else if all_yields.len() == 1 {
                    // Only one incoming edge - use values directly
                    let (yields, _) = &all_yields[0];
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < yields.len() {
                            self.set_value(*output_id, yields[i]);
                        }
                    }
                } else {
                    // No incoming edges - all branches terminated early
                    // Set outputs to undef values for unreachable code that may reference them
                    for output_id in outputs.iter() {
                        self.set_value(
                            *output_id,
                            context.word_type().get_undef().as_basic_value_enum(),
                        );
                    }
                    // Add unreachable terminator
                    context
                        .builder()
                        .build_unreachable()
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    // Create a new block for any dead code that follows
                    let dead_block = context.append_basic_block("switch_dead");
                    context.set_basic_block(dead_block);
                }
            }

            Statement::For {
                init_values,
                loop_vars,
                condition_stmts,
                condition,
                body,
                post,
                outputs,
            } => {
                // Get initial values for loop variables (ensure word type for phi nodes)
                let mut init_llvm_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, init_val) in init_values.iter().enumerate() {
                    init_llvm_values.push(self.translate_value_as_word(
                        init_val,
                        context,
                        &format!("for_init_{}", i),
                    )?);
                }

                let entry_block = context.basic_block();
                let cond_block = context.append_basic_block("for_cond");
                let body_block = context.append_basic_block("for_body");
                let post_block = context.append_basic_block("for_post");
                let join_block = context.append_basic_block("for_join");

                context.build_unconditional_branch(cond_block);
                context.set_basic_block(cond_block);

                // Create phi nodes for loop-carried variables
                let mut loop_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                for (i, loop_var) in loop_vars.iter().enumerate() {
                    let phi = context
                        .builder()
                        .build_phi(context.word_type(), &format!("loop_var_{}", i))
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    // Add incoming from entry (initial value)
                    if i < init_llvm_values.len() {
                        phi.add_incoming(&[(&init_llvm_values[i], entry_block)]);
                    }

                    // Make the phi value available as the loop variable
                    self.set_value(*loop_var, phi.as_basic_value());
                    loop_phis.push(phi);
                }

                // Generate condition statements (these may use loop_vars)
                for stmt in condition_stmts {
                    self.generate_statement(stmt, context)?;
                }

                // Evaluate condition
                let cond_val = self.generate_expr(condition, context)?;
                // Ensure condition is word type before comparing
                let cond_val =
                    self.ensure_word_type(context, cond_val.into_int_value(), "for_cond")?;
                let cond_bool = context
                    .builder()
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        cond_val,
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

                // Collect yields from post region and wire up phi nodes (ensure word type)
                let post_end_block = context.basic_block();
                for (i, phi) in loop_phis.iter().enumerate() {
                    if i < post.yields.len() {
                        let yield_val = self.translate_value_as_word(
                            &post.yields[i],
                            context,
                            &format!("for_post_yield_{}", i),
                        )?;
                        phi.add_incoming(&[(&yield_val, post_end_block)]);
                    }
                }

                context.build_unconditional_branch(cond_block);

                context.pop_loop();
                context.set_basic_block(join_block);

                // Set outputs to the final phi values (values when loop exits)
                for (i, output_id) in outputs.iter().enumerate() {
                    if i < loop_phis.len() {
                        self.set_value(*output_id, loop_phis[i].as_basic_value());
                    }
                }
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

            Statement::Leave { return_values } => {
                // Store return values to return pointer before branching to return block
                match context.current_function().borrow().r#return() {
                    revive_llvm_context::PolkaVMFunctionReturn::None => {}
                    revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                        // Single return value
                        if !return_values.is_empty() {
                            if let Ok(ret_val) = self.translate_value(&return_values[0]) {
                                context.build_store(pointer, ret_val)?;
                            }
                        }
                    }
                    revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                        // Multiple return values - build a struct
                        let field_types: Vec<_> = (0..size)
                            .map(|_| context.word_type().as_basic_type_enum())
                            .collect();
                        let struct_type = context.structure_type(&field_types);
                        let mut struct_val = struct_type.get_undef();
                        for (i, ret_val) in return_values.iter().enumerate() {
                            if let Ok(val) = self.translate_value(ret_val) {
                                struct_val = context
                                    .builder()
                                    .build_insert_value(struct_val, val, i as u32, "ret_insert")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .into_struct_value();
                            }
                        }
                        context.build_store(pointer, struct_val.as_basic_value_enum())?;
                    }
                }
                let return_block = context.current_function().borrow().return_block();
                context.build_unconditional_branch(return_block);
                let unreachable = context.append_basic_block("leave_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Revert { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.ensure_word_type(context, offset_val, "revert_offset")?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.ensure_word_type(context, length_val, "revert_length")?;
                revive_llvm_context::polkavm_evm_return::revert(context, offset_val, length_val)?;
            }

            Statement::Return { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.ensure_word_type(context, offset_val, "return_offset")?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.ensure_word_type(context, length_val, "return_length")?;
                revive_llvm_context::polkavm_evm_return::r#return(context, offset_val, length_val)?;
            }

            Statement::Stop => {
                revive_llvm_context::polkavm_evm_return::stop(context)?;
            }

            Statement::Invalid => {
                revive_llvm_context::polkavm_evm_return::invalid(context)?;
            }

            Statement::SelfDestruct { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "selfdestruct_addr")?;
                revive_llvm_context::polkavm_evm_return::selfdestruct(context, addr_val)?;
            }

            Statement::ExternalCall {
                kind,
                gas,
                address,
                value,
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                result,
            } => {
                // CALLCODE is not supported on PolkaVM
                if matches!(kind, CallKind::CallCode) {
                    return Err(CodegenError::Unsupported(
                        "The `CALLCODE` instruction is not supported".into(),
                    ));
                }

                let gas_val = self.translate_value(gas)?.into_int_value();
                let gas_val = self.ensure_word_type(context, gas_val, "call_gas")?;
                let address_val = self.translate_value(address)?.into_int_value();
                let address_val = self.ensure_word_type(context, address_val, "call_addr")?;
                let args_offset_val = self.translate_value(args_offset)?.into_int_value();
                let args_length_val = self.translate_value(args_length)?.into_int_value();
                let ret_offset_val = self.translate_value(ret_offset)?.into_int_value();
                let ret_length_val = self.translate_value(ret_length)?.into_int_value();

                let call_result = match kind {
                    CallKind::Call => {
                        let value_val = value
                            .map(|v| -> Result<_> {
                                let val = self.translate_value(&v)?.into_int_value();
                                self.ensure_word_type(context, val, "call_value")
                            })
                            .transpose()?;
                        revive_llvm_context::polkavm_evm_call::call(
                            context,
                            gas_val,
                            address_val,
                            value_val,
                            args_offset_val,
                            args_length_val,
                            ret_offset_val,
                            ret_length_val,
                            vec![], // No constant simulation addresses
                            false,  // Not a static call
                        )?
                    }
                    CallKind::CallCode => {
                        unreachable!("CallCode is handled above")
                    }
                    CallKind::StaticCall => {
                        revive_llvm_context::polkavm_evm_call::call(
                            context,
                            gas_val,
                            address_val,
                            None,
                            args_offset_val,
                            args_length_val,
                            ret_offset_val,
                            ret_length_val,
                            vec![], // No constant simulation addresses
                            true,   // Static call
                        )?
                    }
                    CallKind::DelegateCall => {
                        revive_llvm_context::polkavm_evm_call::delegate_call(
                            context,
                            gas_val,
                            address_val,
                            args_offset_val,
                            args_length_val,
                            ret_offset_val,
                            ret_length_val,
                            vec![], // No constant simulation addresses
                        )?
                    }
                };
                self.set_value(*result, call_result);
            }

            Statement::Create {
                kind,
                value,
                offset,
                length,
                salt,
                result,
            } => {
                let value_val = self.translate_value(value)?.into_int_value();
                let value_val = self.ensure_word_type(context, value_val, "create_value")?;
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                let salt_val = match (kind, salt) {
                    (CreateKind::Create2, Some(s)) => {
                        let s_val = self.translate_value(s)?.into_int_value();
                        Some(self.ensure_word_type(context, s_val, "create_salt")?)
                    }
                    _ => None,
                };

                let create_result = revive_llvm_context::polkavm_evm_create::create(
                    context, value_val, offset_val, length_val, salt_val,
                )?;
                self.set_value(*result, create_result);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                // Topics must be word type for the log function
                let topic_vals: Vec<BasicValueEnum<'ctx>> = topics
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let val = self.translate_value(t)?.into_int_value();
                        let val =
                            self.ensure_word_type(context, val, &format!("log_topic_{}", i))?;
                        Ok(val.as_basic_value_enum())
                    })
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

            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => {
                // CODECOPY is only supported in deploy code, not runtime
                if matches!(
                    context.code_type(),
                    Some(revive_llvm_context::PolkaVMCodeType::Runtime)
                ) {
                    return Err(CodegenError::Unsupported(
                        "The `CODECOPY` instruction is not supported in the runtime code".into(),
                    ));
                }
                // In deploy code, codecopy copies from calldata
                let dest_val = self.translate_value(dest)?.into_int_value();
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                revive_llvm_context::polkavm_evm_calldata::copy(
                    context, dest_val, offset_val, length_val,
                )?;
            }

            Statement::ExtCodeCopy { .. } => {
                // ExtCodeCopy is not supported on PolkaVM
                return Err(CodegenError::Unsupported(
                    "The `EXTCODECOPY` instruction is not supported".into(),
                ));
            }

            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => {
                let dest_val = self.translate_value(dest)?.into_int_value();
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                revive_llvm_context::polkavm_evm_return_data::copy(
                    context, dest_val, offset_val, length_val,
                )?;
            }

            Statement::DataCopy {
                dest,
                offset,
                length: _,
            } => {
                // DataCopy writes the contract hash at the destination offset
                // This is used in create patterns: datacopy(dest, dataoffset("deployed"), datasize("deployed"))
                // The offset is actually the contract hash to write
                let dest_val = self.translate_value(dest)?.into_int_value();
                let hash_val = self.translate_value(offset)?.into_int_value();
                let hash_val = self.ensure_word_type(context, hash_val, "datacopy_hash")?;
                revive_llvm_context::polkavm_evm_memory::store(context, dest_val, hash_val)?;
            }

            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => {
                let dest_val = self.translate_value(dest)?.into_int_value();
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();
                revive_llvm_context::polkavm_evm_calldata::copy(
                    context, dest_val, offset_val, length_val,
                )?;
            }

            Statement::Block(region) => {
                self.generate_region(region, context)?;
            }

            Statement::Expr(expr) => {
                // Evaluate for side effects, discard result
                let _ = self.generate_expr(expr, context)?;
            }

            Statement::SetImmutable { key, value } => {
                let offset = context.solidity_mut().allocate_immutable(key.as_str())
                    / revive_common::BYTE_LENGTH_WORD;
                let index = context.xlen_type().const_int(offset as u64, false);
                let val = self.translate_value(value)?.into_int_value();
                let val = self.ensure_word_type(context, val, "immutable_val")?;
                revive_llvm_context::polkavm_evm_immutable::store(context, index, val)?;
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
            Expr::Literal { value, .. } => {
                // Generate narrow literals for values that fit in 64 bits.
                // This enables more efficient code generation - the values will be
                // extended to word type only where needed (runtime function calls).
                if value.bits() <= 64 {
                    let val_u64 = value.to_u64().unwrap_or(0);
                    Ok(context
                        .llvm()
                        .i64_type()
                        .const_int(val_u64, false)
                        .as_basic_value_enum())
                } else {
                    let val_str = value.to_string();
                    Ok(context.word_const_str_dec(&val_str).as_basic_value_enum())
                }
            }

            Expr::Var(id) => self.get_value(*id),

            Expr::Binary { op, lhs, rhs } => {
                let lhs_val = self.translate_value(lhs)?.into_int_value();
                let rhs_val = self.translate_value(rhs)?.into_int_value();

                // Type inference Phase 2: Use narrow types where possible.
                // - Comparisons: can always operate on narrow types (result is i1)
                // - Simple arithmetic (add, sub, mul): can use narrow types when both
                //   operands are already narrow (LLVM handles wrapping correctly)
                // - Bitwise ops (and, or, xor): can use narrow types
                // - Shifts, division, modulo, exp: require word type for EVM semantics
                //   (shift functions use word_const, division has special zero handling)
                match op {
                    // Comparisons: operate on narrow types directly
                    BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt | BinOp::Eq => {
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "cmp")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // Simple arithmetic: use narrow types when possible
                    // LLVM's add/sub/mul with wrapping has same semantics at any width
                    BinOp::Add | BinOp::Sub | BinOp::Mul => {
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "arith")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // Bitwise operations: can use narrow types
                    BinOp::And | BinOp::Or | BinOp::Xor => {
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "bitwise")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // All other operations require word type for EVM semantics:
                    // - Shifts: runtime functions use word_const and word_type
                    // - Division/Mod: runtime functions for div-by-zero handling
                    // - Exp: can overflow to full 256-bit
                    // - Byte/SignExtend: operate on 256-bit words
                    _ => {
                        let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                        let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }
                }
            }

            Expr::Ternary { op, a, b, n } => {
                let a_val = self.translate_value(a)?.into_int_value();
                let b_val = self.translate_value(b)?.into_int_value();
                let n_val = self.translate_value(n)?.into_int_value();
                // Ensure all operands are word type for ternary operations
                let a_val = self.ensure_word_type(context, a_val, "ternary_a")?;
                let b_val = self.ensure_word_type(context, b_val, "ternary_b")?;
                let n_val = self.ensure_word_type(context, n_val, "ternary_n")?;

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
                        // IsZero: EVM returns 1 if operand == 0, else 0 (as 256-bit word)
                        let zero = operand_val.get_type().const_zero();
                        let is_zero = context
                            .builder()
                            .build_int_compare(
                                inkwell::IntPredicate::EQ,
                                operand_val,
                                zero,
                                "iszero",
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        // Zero-extend i1 to word type for EVM compatibility
                        let result = context
                            .builder()
                            .build_int_z_extend(is_zero, context.word_type(), "iszero_ext")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        Ok(result.as_basic_value_enum())
                    }
                    UnaryOp::Not => {
                        // Bitwise NOT: XOR with all-ones mask at word (256-bit) width.
                        // This matches the YUL backend behavior and ensures the NOT
                        // operates at full EVM word width regardless of operand type.
                        let operand_val = self.ensure_word_type(context, operand_val, "not_op")?;
                        let all_ones = context.word_type().const_all_ones();
                        let xor_result = context
                            .builder()
                            .build_xor(operand_val, all_ones, "not_result")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        Ok(xor_result.as_basic_value_enum())
                    }
                    UnaryOp::Clz => {
                        // Count leading zeros requires word type
                        let operand_val = self.ensure_word_type(context, operand_val, "clz_op")?;
                        Ok(
                            revive_llvm_context::polkavm_evm_bitwise::count_leading_zeros(
                                context,
                                operand_val,
                            )?,
                        )
                    }
                }
            }

            Expr::CallDataLoad { offset } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                Ok(revive_llvm_context::polkavm_evm_calldata::load(
                    context, offset_val,
                )?)
            }

            Expr::CallValue => Ok(revive_llvm_context::polkavm_evm_ether_gas::value(context)?),

            Expr::Caller => Ok(revive_llvm_context::polkavm_evm_contract_context::caller(
                context,
            )?),

            Expr::Origin => Ok(revive_llvm_context::polkavm_evm_contract_context::origin(
                context,
            )?),

            Expr::CallDataSize => Ok(revive_llvm_context::polkavm_evm_calldata::size(context)?),

            Expr::CodeSize => match context.code_type() {
                Some(revive_llvm_context::PolkaVMCodeType::Deploy) => {
                    Ok(revive_llvm_context::polkavm_evm_calldata::size(context)?)
                }
                Some(revive_llvm_context::PolkaVMCodeType::Runtime) => Ok(
                    revive_llvm_context::polkavm_evm_ext_code::size(context, None)?,
                ),
                None => Err(CodegenError::Unsupported(
                    "code type undefined for codesize".into(),
                )),
            },

            Expr::GasPrice => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_price(context)?)
            }

            Expr::ExtCodeSize { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "extcodesize_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::size(
                    context,
                    Some(addr_val),
                )?)
            }

            Expr::ReturnDataSize => {
                Ok(revive_llvm_context::polkavm_evm_return_data::size(context)?)
            }

            Expr::ExtCodeHash { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "extcodehash_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::hash(
                    context, addr_val,
                )?)
            }

            Expr::BlockHash { number } => {
                let num_val = self.translate_value(number)?.into_int_value();
                let num_val = self.ensure_word_type(context, num_val, "blockhash_num")?;
                Ok(
                    revive_llvm_context::polkavm_evm_contract_context::block_hash(
                        context, num_val,
                    )?,
                )
            }

            Expr::Coinbase => Ok(revive_llvm_context::polkavm_evm_contract_context::coinbase(
                context,
            )?),

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

            Expr::ChainId => Ok(revive_llvm_context::polkavm_evm_contract_context::chain_id(
                context,
            )?),

            Expr::SelfBalance => Ok(revive_llvm_context::polkavm_evm_ether_gas::self_balance(
                context,
            )?),

            Expr::BaseFee => Ok(revive_llvm_context::polkavm_evm_contract_context::basefee(
                context,
            )?),

            Expr::BlobHash { .. } | Expr::BlobBaseFee => {
                // Blob opcodes return 0 for now (EIP-4844)
                Ok(context.word_const(0).as_basic_value_enum())
            }

            Expr::Gas => Ok(revive_llvm_context::polkavm_evm_ether_gas::gas(context)?),

            Expr::MSize => Ok(revive_llvm_context::polkavm_evm_memory::msize(context)?),

            Expr::Address => Ok(revive_llvm_context::polkavm_evm_contract_context::address(
                context,
            )?),

            Expr::Balance { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "balance_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ether_gas::balance(
                    context, addr_val,
                )?)
            }

            Expr::MLoad { offset, region: _ } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                if self.can_use_native_memory(offset_val) {
                    // Native memory operations require 32-bit offset (xlen_type)
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                    if self.is_all_native() {
                        // All accesses native: use runtime function (function body is emitted)
                        Ok(revive_llvm_context::polkavm_evm_memory::load_native(
                            context,
                            offset_xlen,
                        )?)
                    } else {
                        // Per-access native: inline the load (no function overhead)
                        Ok(revive_llvm_context::polkavm_evm_memory::load_native_inline(
                            context,
                            offset_xlen,
                        )?)
                    }
                } else {
                    Ok(revive_llvm_context::polkavm_evm_memory::load(
                        context, offset_val,
                    )?)
                }
            }

            Expr::SLoad {
                key,
                static_slot: _,
            } => {
                let key_arg = self.value_to_argument(key, context)?;
                Ok(revive_llvm_context::polkavm_evm_storage::load(
                    context, &key_arg,
                )?)
            }

            Expr::TLoad { key } => {
                let key_arg = self.value_to_argument(key, context)?;
                Ok(revive_llvm_context::polkavm_evm_storage::transient_load(
                    context, &key_arg,
                )?)
            }

            Expr::Call { function, args } => {
                let func_name = self
                    .function_names
                    .get(&function.0)
                    .ok_or(CodegenError::UndefinedFunction(*function))?
                    .clone();

                // Ensure all arguments are word type (function parameters expect word type)
                let mut arg_vals = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    arg_vals.push(self.translate_value_as_word(
                        arg,
                        context,
                        &format!("call_arg_{}", i),
                    )?);
                }

                // Ensure debug location is set for function calls
                context.set_debug_location(1, 0, None)?;

                let func = context
                    .get_function(&func_name, true)
                    .ok_or(CodegenError::UndefinedFunction(*function))?;
                let result = context.build_call(
                    func.borrow().declaration(),
                    &arg_vals,
                    &format!("{}_result", func_name),
                );

                // build_call returns None for void functions, which is valid
                // For void functions, return a zero value as a placeholder
                Ok(result.unwrap_or_else(|| context.word_const(0).as_basic_value_enum()))
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
                    context, offset_val, length_val,
                )?)
            }

            Expr::DataOffset { id } => {
                // DataOffset returns a reference to the contract code hash
                // For subcontract deployments, this is the hash of the deployed bytecode
                // We use the PolkaVM create infrastructure which handles this
                let arg =
                    revive_llvm_context::polkavm_evm_create::contract_hash(context, id.clone())?;
                arg.access(context)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))
            }

            Expr::DataSize { id } => {
                // DataSize returns the size of the deploy call header
                let arg =
                    revive_llvm_context::polkavm_evm_create::header_size(context, id.clone())?;
                arg.access(context)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))
            }

            Expr::LoadImmutable { key } => {
                let offset = context
                    .solidity_mut()
                    .get_or_allocate_immutable(key.as_str())
                    / revive_common::BYTE_LENGTH_WORD;
                let index = context.xlen_type().const_int(offset as u64, false);
                Ok(revive_llvm_context::polkavm_evm_immutable::load(
                    context, index,
                )?)
            }

            Expr::LinkerSymbol { path } => Ok(
                revive_llvm_context::polkavm_evm_call::linker_symbol(context, path)?,
            ),
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
            BinOp::Add => Ok(revive_llvm_context::polkavm_evm_arithmetic::addition(
                context, lhs, rhs,
            )?),
            BinOp::Sub => Ok(revive_llvm_context::polkavm_evm_arithmetic::subtraction(
                context, lhs, rhs,
            )?),
            BinOp::Mul => Ok(revive_llvm_context::polkavm_evm_arithmetic::multiplication(
                context, lhs, rhs,
            )?),
            BinOp::Div => Ok(revive_llvm_context::polkavm_evm_arithmetic::division(
                context, lhs, rhs,
            )?),
            BinOp::SDiv => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::division_signed(context, lhs, rhs)?,
            ),
            BinOp::Mod => Ok(revive_llvm_context::polkavm_evm_arithmetic::remainder(
                context, lhs, rhs,
            )?),
            BinOp::SMod => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::remainder_signed(context, lhs, rhs)?,
            ),
            BinOp::Exp => Ok(revive_llvm_context::polkavm_evm_math::exponent(
                context, lhs, rhs,
            )?),
            BinOp::And => Ok(revive_llvm_context::polkavm_evm_bitwise::and(
                context, lhs, rhs,
            )?),
            BinOp::Or => Ok(revive_llvm_context::polkavm_evm_bitwise::or(
                context, lhs, rhs,
            )?),
            BinOp::Xor => Ok(revive_llvm_context::polkavm_evm_bitwise::xor(
                context, lhs, rhs,
            )?),
            BinOp::Shl => Ok(revive_llvm_context::polkavm_evm_bitwise::shift_left(
                context, lhs, rhs,
            )?),
            BinOp::Shr => Ok(revive_llvm_context::polkavm_evm_bitwise::shift_right(
                context, lhs, rhs,
            )?),
            BinOp::Sar => Ok(
                revive_llvm_context::polkavm_evm_bitwise::shift_right_arithmetic(
                    context, lhs, rhs,
                )?,
            ),
            BinOp::Lt => {
                // EVM comparison: result is 0 or 1 as 256-bit word
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::ULT, lhs, rhs, "lt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                let result = context
                    .builder()
                    .build_int_z_extend(cmp, context.word_type(), "lt_ext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(result.as_basic_value_enum())
            }
            BinOp::Gt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::UGT, lhs, rhs, "gt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                let result = context
                    .builder()
                    .build_int_z_extend(cmp, context.word_type(), "gt_ext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(result.as_basic_value_enum())
            }
            BinOp::Slt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::SLT, lhs, rhs, "slt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                let result = context
                    .builder()
                    .build_int_z_extend(cmp, context.word_type(), "slt_ext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(result.as_basic_value_enum())
            }
            BinOp::Sgt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::SGT, lhs, rhs, "sgt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                let result = context
                    .builder()
                    .build_int_z_extend(cmp, context.word_type(), "sgt_ext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(result.as_basic_value_enum())
            }
            BinOp::Eq => {
                // Equal comparison: result is 0 or 1 as 256-bit word
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
            BinOp::Byte => Ok(revive_llvm_context::polkavm_evm_bitwise::byte(
                context, lhs, rhs,
            )?),
            BinOp::SignExtend => Ok(revive_llvm_context::polkavm_evm_math::sign_extend(
                context, lhs, rhs,
            )?),
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

    /// Translates a value and ensures it's word type.
    /// Used for phi nodes and other operations requiring consistent types.
    fn translate_value_as_word(
        &self,
        value: &Value,
        context: &PolkaVMContext<'ctx>,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>> {
        let llvm_val = self.translate_value(value)?;
        if llvm_val.is_int_value() {
            let int_val = llvm_val.into_int_value();
            Ok(self
                .ensure_word_type(context, int_val, name)?
                .as_basic_value_enum())
        } else {
            Ok(llvm_val)
        }
    }

    /// Converts a Value to a PolkaVMArgument for storage operations.
    /// Storage operations require 256-bit values, so narrow values are zero-extended.
    fn value_to_argument(
        &self,
        value: &Value,
        context: &PolkaVMContext<'ctx>,
    ) -> Result<PolkaVMArgument<'ctx>> {
        let llvm_val = self.translate_value(value)?;
        if llvm_val.is_int_value() {
            let int_val = llvm_val.into_int_value();
            let word_val = self.ensure_word_type(context, int_val, "storage_arg")?;
            Ok(PolkaVMArgument::value(word_val.as_basic_value_enum()))
        } else {
            Ok(PolkaVMArgument::value(llvm_val))
        }
    }
}

impl Default for LlvmCodegen<'_> {
    fn default() -> Self {
        Self::new(HeapOptResults::default(), TypeInference::default())
    }
}
