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
    BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId, MemoryRegion, Object,
    Region, Statement, Type, UnaryOp, Value, ValueId,
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

/// Functions with size_estimate at or above this threshold get NoInline when appropriate.
/// This prevents code bloat from inlining large function bodies at multiple call sites.
const LARGE_FUNCTION_NOINLINE_THRESHOLD: usize = 50;

/// Functions with size_estimate at or below this threshold get AlwaysInline when no
/// IR-level decision was made. Very small functions benefit from inlining.
const SMALL_FUNCTION_ALWAYSINLINE_THRESHOLD: usize = 8;

/// Maximum function size for AlwaysInline when called exactly twice (CostBenefit).
/// Functions larger than this are left to LLVM's judgment or marked NoInline to avoid
/// code duplication that outweighs interprocedural optimization benefits.
const COST_BENEFIT_INLINE_SIZE_LIMIT: usize = 20;

/// LLVM code generator for newyork IR.
/// Tracks phi nodes at the continue-landing block of a for loop.
/// These phi nodes merge values from the body's normal exit and from continue sites.
struct ForLoopPostPhis<'ctx> {
    /// Phi nodes at the continue-landing block, one per body yield.
    /// The phi merges body-end values with continue-site values.
    phis: Vec<inkwell::values::PhiValue<'ctx>>,
    /// The loop-carried variable phi values (from the cond phi nodes).
    /// Used as fallback values when continue is taken before a body yield is defined.
    loop_var_phi_values: Vec<BasicValueEnum<'ctx>>,
}

/// Tracks phi nodes at the join block of a for loop.
/// These merge values from the normal loop exit (condition false) and break sites.
struct ForLoopBreakPhis<'ctx> {
    /// Phi nodes at the join block, one per loop-carried variable.
    phis: Vec<inkwell::values::PhiValue<'ctx>>,
    /// The loop-carried variable phi values (from the cond phi nodes).
    /// Used as fallback values when break is taken before a body yield is defined.
    loop_var_phi_values: Vec<BasicValueEnum<'ctx>>,
}

pub struct LlvmCodegen<'ctx> {
    /// Value table: maps IR ValueId to LLVM value.
    values: BTreeMap<u32, BasicValueEnum<'ctx>>,
    /// Function table: maps IR FunctionId to function name.
    function_names: BTreeMap<u32, String>,
    /// Function parameter types: maps IR FunctionId to parameter types.
    /// Used by call sites to match argument types to narrowed parameter types.
    function_param_types: BTreeMap<u32, Vec<Type>>,
    /// Set of function names that have already been generated.
    /// This is used to avoid regenerating shared utility functions in multi-contract scenarios.
    generated_functions: BTreeSet<String>,
    /// Heap optimization results for skipping byte-swapping on internal memory.
    heap_opt: HeapOptResults,
    /// Type inference results for using narrower types.
    type_info: TypeInference,
    /// Inline decisions from our IR-level inliner.
    /// Used to guide LLVM's inliner for functions that couldn't be inlined at IR level.
    inline_decisions: BTreeMap<u32, crate::InlineDecision>,
    /// Stack of for-loop post-block phi info, for continue to contribute values.
    for_loop_post_phis: Vec<ForLoopPostPhis<'ctx>>,
    /// Stack of for-loop join-block phi info, for break to contribute values.
    for_loop_break_phis: Vec<ForLoopBreakPhis<'ctx>>,
    /// Shared basic blocks for `revert(0, K)` patterns (keyed by constant length K).
    /// Each block contains the full exit sequence for that revert pattern.
    /// Created on first use within each entry point function (deploy/runtime).
    revert_blocks: BTreeMap<u64, inkwell::basic_block::BasicBlock<'ctx>>,
    /// Shared basic blocks for `return(offset, length)` patterns where both are constants.
    /// Keyed by `(offset, length)` pair. Created on first use within each entry point.
    return_blocks: BTreeMap<(u64, u64), inkwell::basic_block::BasicBlock<'ctx>>,
    /// Shared basic blocks for Solidity panic revert patterns (keyed by error code).
    /// Each block stores the Panic(uint256) ABI encoding and branches to revert(0, 0x24).
    panic_blocks: BTreeMap<u8, inkwell::basic_block::BasicBlock<'ctx>>,
    /// Whether to use the outlined `__revive_callvalue()` runtime function.
    /// True when the contract has enough callvalue sites for outlining to pay off.
    use_outlined_callvalue: bool,
    /// Whether to use the outlined `__revive_calldataload()` runtime function.
    /// True when the contract has enough calldataload sites for outlining to pay off.
    use_outlined_calldataload: bool,
    /// Whether to use the outlined `__revive_caller()` runtime function.
    /// True when the contract has enough caller sites for outlining to pay off.
    use_outlined_caller: bool,
}

impl<'ctx> LlvmCodegen<'ctx> {
    /// Creates a new code generator with optimization results.
    pub fn new(
        heap_opt: HeapOptResults,
        type_info: TypeInference,
        inline_decisions: BTreeMap<u32, crate::InlineDecision>,
    ) -> Self {
        LlvmCodegen {
            values: BTreeMap::new(),
            function_names: BTreeMap::new(),
            function_param_types: BTreeMap::new(),
            generated_functions: BTreeSet::new(),
            heap_opt,
            type_info,
            inline_decisions,
            for_loop_post_phis: Vec::new(),
            for_loop_break_phis: Vec::new(),
            revert_blocks: BTreeMap::new(),
            return_blocks: BTreeMap::new(),
            panic_blocks: BTreeMap::new(),
            use_outlined_callvalue: false,
            use_outlined_calldataload: false,
            use_outlined_caller: false,
        }
    }

    /// Creates a new code generator that shares the generated_functions set with another.
    /// This is used for subobjects to avoid regenerating shared utility functions.
    pub fn new_with_shared_functions(
        generated_functions: BTreeSet<String>,
        heap_opt: HeapOptResults,
        type_info: TypeInference,
        inline_decisions: BTreeMap<u32, crate::InlineDecision>,
    ) -> Self {
        LlvmCodegen {
            values: BTreeMap::new(),
            function_names: BTreeMap::new(),
            function_param_types: BTreeMap::new(),
            generated_functions,
            heap_opt,
            type_info,
            inline_decisions,
            for_loop_post_phis: Vec::new(),
            for_loop_break_phis: Vec::new(),
            revert_blocks: BTreeMap::new(),
            return_blocks: BTreeMap::new(),
            panic_blocks: BTreeMap::new(),
            use_outlined_callvalue: false,
            use_outlined_calldataload: false,
            use_outlined_caller: false,
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

    /// Tries to extract a constant u64 from an LLVM IntValue.
    /// Returns Some(value) if the value is a constant that fits in u64.
    /// Handles i256 constants correctly (where `get_zero_extended_constant()` returns None
    /// even for small values because the bit-width exceeds 64).
    fn try_extract_const_u64(value: IntValue<'ctx>) -> Option<u64> {
        if !value.is_const() {
            return None;
        }

        // Try to get as zero-extended constant (works for <= 64-bit types)
        if let Some(v) = value.get_zero_extended_constant() {
            return Some(v);
        }

        // For i256 constants, get_zero_extended_constant returns None because the
        // bit-width > 64. Parse the printed representation instead.
        let s = value.print_to_string().to_string();
        // Format is "i256 <value>" - extract the value part
        if let Some(val_str) = s.strip_prefix("i256 ") {
            if let Ok(v) = val_str.trim().parse::<u64>() {
                return Some(v);
            }
        }

        None
    }

    /// Checks if an LLVM IntValue is a constant zero, handling all bit widths including i256.
    fn is_const_zero(value: IntValue<'ctx>) -> bool {
        Self::try_extract_const_u64(value) == Some(0)
    }

    /// Checks if an MLoad is loading the free memory pointer (offset 0x40).
    /// The free memory pointer is a Solidity convention where mload(64) returns
    /// the next free heap address. This value is always < 2^32 on PolkaVM.
    fn is_free_pointer_load(offset: IntValue<'ctx>) -> bool {
        Self::try_extract_const_u64(offset) == Some(0x40)
    }

    /// Applies a range proof to a value by truncating to a narrower type and zero-extending
    /// back to word type. This proves to LLVM that the value fits in the narrow type,
    /// enabling overflow check elimination and arithmetic simplification downstream.
    fn apply_range_proof(
        context: &PolkaVMContext<'ctx>,
        value: BasicValueEnum<'ctx>,
        narrow_bits: u32,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>> {
        let value_int = value.into_int_value();
        let narrow_type = context.integer_type(narrow_bits as usize);
        let truncated = context
            .builder()
            .build_int_truncate(value_int, narrow_type, &format!("{name}_narrow"))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let extended = context
            .builder()
            .build_int_z_extend(truncated, context.word_type(), &format!("{name}_extend"))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        Ok(extended.as_basic_value_enum())
    }

    /// Checks if a memory operation at the given offset can use native byte order.
    ///
    /// Only enables native memory when ALL accesses are safe (all_native mode).
    /// This avoids defining both native and byte-swapping functions.
    ///
    /// Per-access native mode is disabled: mixing native and byte-swapped
    /// operations at different offsets is fragile and hard to verify correct
    /// in all cases (aliasing, control flow, subobjects).
    #[allow(unused_variables)]
    fn can_use_native_memory(&self, offset: IntValue<'ctx>) -> bool {
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

    /// Gets the inferred bit-width for a value from type inference.
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

    /// Ensures both operands are extended to at least the given minimum width.
    /// This is used for arithmetic operations where the result needs a certain
    /// width to avoid modular arithmetic wrapping at the wrong boundary.
    fn ensure_min_width(
        &self,
        context: &PolkaVMContext<'ctx>,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
        min_width: u32,
        name: &str,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let target_width = min_width
            .max(a.get_type().get_bit_width())
            .max(b.get_type().get_bit_width());
        let target_type = context.integer_type(target_width as usize);

        let a_ext = if a.get_type().get_bit_width() < target_width {
            context
                .builder()
                .build_int_z_extend(a, target_type, &format!("{}_ext_a", name))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else {
            a
        };
        let b_ext = if b.get_type().get_bit_width() < target_width {
            context
                .builder()
                .build_int_z_extend(b, target_type, &format!("{}_ext_b", name))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else {
            b
        };
        Ok((a_ext, b_ext))
    }

    /// Narrows a Let-bound value to i64 when type inference proves it fits.
    /// This enables native RISC-V arithmetic for downstream operations.
    ///
    /// Safety: arithmetic operations (Add/Sub/Mul) ensure operands are
    /// extended to the result's inferred width before operating, so narrow
    /// values cannot cause incorrect modular wrapping.
    fn try_narrow_let_binding(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: BasicValueEnum<'ctx>,
        binding_id: ValueId,
    ) -> Result<BasicValueEnum<'ctx>> {
        let int_val = match value {
            BasicValueEnum::IntValue(v) => v,
            _ => return Ok(value),
        };

        let value_width = int_val.get_type().get_bit_width();

        // Only truncate values wider than 64 bits
        if value_width <= 64 {
            return Ok(value);
        }

        let constraint = self.type_info.get(binding_id);

        // Don't narrow signed values — truncation followed by zero-extension
        // doesn't preserve sign information for negative values.
        if constraint.is_signed {
            return Ok(value);
        }

        if constraint.min_width.bits() > 64 {
            return Ok(value);
        }

        // Check if the value already has a range proof from a zext instruction.
        // For example, the FMP range proof emits `trunc i256 → i32; zext i32 → i256`.
        // If we blindly truncate to i64, we lose the tighter 32-bit constraint.
        // Preserve the tightest existing proof by truncating to the source width.
        let existing_proof_width = Self::detect_zext_source_width(int_val);
        if let Some(src_width) = existing_proof_width {
            if src_width <= 64 {
                // Value already has a range proof at src_width bits.
                // Truncate to that width to preserve the tight constraint.
                let narrow_type = context.integer_type(src_width as usize);
                let truncated = context
                    .builder()
                    .build_int_truncate(int_val, narrow_type, "narrow_let")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                return Ok(truncated.as_basic_value_enum());
            }
        }

        // Default: truncate to i64 to avoid proliferating many small LLVM types.
        let i64_type = context.llvm().i64_type();
        let truncated = context
            .builder()
            .build_int_truncate(int_val, i64_type, "narrow_let")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        Ok(truncated.as_basic_value_enum())
    }

    /// Detects if an IntValue was produced by a ZExt instruction and returns the
    /// source type's bit width. This preserves range proofs: if a value is
    /// `zext i32 → i256`, the source width is 32.
    fn detect_zext_source_width(value: IntValue<'ctx>) -> Option<u32> {
        use inkwell::values::InstructionOpcode;
        let instruction = value.as_instruction_value()?;
        if instruction.get_opcode() != InstructionOpcode::ZExt {
            return None;
        }
        let operand = instruction.get_operand(0)?.value()?;
        Some(operand.into_int_value().get_type().get_bit_width())
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

    /// Narrows a memory offset or length value to i64 when type inference
    /// proves the value fits. This eliminates the expensive 3-basic-block
    /// overflow check in `safe_truncate_int_to_xlen` (which converts i256→xlen)
    /// by giving it an i64 input that takes the cheap direct truncation path.
    ///
    /// This is only applied at memory operation USE sites (mstore, mload,
    /// return, revert, etc.) — never at LET bindings — so it cannot affect
    /// intermediate arithmetic which must remain at full width.
    fn narrow_offset_for_pointer(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        source_id: ValueId,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let value_width = value.get_type().get_bit_width();
        let word_width = context.word_type().get_bit_width();

        // Only narrow word-sized (i256) values — these are the ones that
        // trigger the expensive overflow check in safe_truncate_int_to_xlen.
        if value_width != word_width {
            return Ok(value);
        }

        let inferred = self.inferred_width(source_id);
        if !matches!(
            inferred,
            BitWidth::I1 | BitWidth::I8 | BitWidth::I32 | BitWidth::I64
        ) {
            return Ok(value);
        }

        // Truncate to i64 — safe because type inference proves the value fits.
        // safe_truncate_int_to_xlen will then do a cheap i64→i32 truncation
        // instead of the expensive i256→i32 overflow check.
        let i64_type = context.llvm().i64_type();
        context
            .builder()
            .build_int_truncate(value, i64_type, name)
            .map_err(|e| CodegenError::Llvm(e.to_string()))
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

    /// Gets or creates a shared basic block for `revert(0, K)` where K is a constant length.
    ///
    /// Many contracts (especially those generated by OpenZeppelin) have dozens or
    /// even hundreds of identical revert sites. Common patterns include:
    /// - `revert(0, 0)`: callvalue checks, ABI decoding checks, overflow guards (100+ sites)
    /// - `revert(0, 36)`: Solidity custom error messages with string (70+ sites)
    /// - `revert(0, 4)`: custom error selectors (20+ sites)
    /// - `revert(0, 68)`: errors with two arguments (17+ sites)
    ///
    /// Each site generates the same exit sequence: safe_truncate(offset) + safe_truncate(length)
    /// + seal_return(1, heap_base+offset, length) + unreachable.
    ///
    /// By creating a single shared block per (offset=0, length=K) pattern and branching to it,
    /// we eliminate the duplication. On OZ ERC20, this saves 200+ copies of these patterns.
    fn get_or_create_revert_block(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
        const_length: u64,
    ) -> Result<inkwell::basic_block::BasicBlock<'ctx>> {
        if let Some(&block) = self.revert_blocks.get(&const_length) {
            return Ok(block);
        }

        // Save current insertion point
        let current_block = context.basic_block();

        // Create the shared revert block at the end of the current function
        let block_name = format!("revert_shared_{const_length}");
        let revert_block = context.append_basic_block(&block_name);
        context.set_basic_block(revert_block);

        // Emit the revert(0, K) exit sequence
        let zero = context.word_const(0);
        let length = context.word_const(const_length);
        revive_llvm_context::polkavm_evm_return::revert(context, zero, length)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Ensure the block has a terminator. The seal_return call is noreturn
        // but LLVM needs an explicit unreachable terminator in the basic block.
        context.build_unreachable();

        // Restore insertion point
        context.set_basic_block(current_block);

        self.revert_blocks.insert(const_length, revert_block);
        Ok(revert_block)
    }

    /// Gets or creates a shared basic block for a Solidity panic revert with a given error code.
    /// The block stores `Panic(uint256)` ABI encoding to scratch memory and reverts.
    /// This deduplicates the common pattern: mstore(0, 0x4e487b71...), mstore(4, code), revert(0, 36).
    fn get_or_create_panic_block(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
        error_code: u8,
    ) -> Result<inkwell::basic_block::BasicBlock<'ctx>> {
        if let Some(&block) = self.panic_blocks.get(&error_code) {
            return Ok(block);
        }

        // Save current insertion point
        let current_block = context.basic_block();

        // Create the shared panic block
        let block_name = format!("panic_0x{error_code:02x}");
        let panic_block = context.append_basic_block(&block_name);
        context.set_basic_block(panic_block);

        // Emit: mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
        let zero_offset = context.word_const(0);
        let panic_selector = context
            .word_const_str_hex("4e487b7100000000000000000000000000000000000000000000000000000000");
        revive_llvm_context::polkavm_evm_memory::store(context, zero_offset, panic_selector)?;

        // Emit: mstore(4, error_code)
        let four_offset = context.word_const(4);
        let code_val = context.word_const(error_code as u64);
        revive_llvm_context::polkavm_evm_memory::store(context, four_offset, code_val)?;

        // Branch to the shared revert(0, 0x24) block
        let revert_block = self.get_or_create_revert_block(context, 0x24)?;
        context.build_unconditional_branch(revert_block);

        // Restore insertion point
        context.set_basic_block(current_block);

        self.panic_blocks.insert(error_code, panic_block);
        Ok(panic_block)
    }

    /// Gets or creates a shared basic block for a `return(offset, length)` pattern
    /// where both offset and length are constants. This deduplicates identical return
    /// sequences (e.g., `return(0x80, 0x20)` appearing 7 times in ERC20 runtime).
    /// Each shared block contains the full exit sequence including `store_immutable_data`
    /// for deploy code.
    fn get_or_create_return_block(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
        const_offset: u64,
        const_length: u64,
    ) -> Result<inkwell::basic_block::BasicBlock<'ctx>> {
        let key = (const_offset, const_length);
        if let Some(&block) = self.return_blocks.get(&key) {
            return Ok(block);
        }

        // Save current insertion point
        let current_block = context.basic_block();

        // Create the shared return block at the end of the current function
        let block_name = format!("return_shared_{const_offset:x}_{const_length:x}");
        let return_block = context.append_basic_block(&block_name);
        context.set_basic_block(return_block);

        // Emit the return(offset, length) exit sequence
        let offset_val = context.word_const(const_offset);
        let length_val = context.word_const(const_length);
        revive_llvm_context::polkavm_evm_return::r#return(context, offset_val, length_val)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Ensure the block has a terminator. The seal_return call is noreturn
        // but LLVM needs an explicit unreachable terminator in the basic block.
        context.build_unreachable();

        // Restore insertion point
        context.set_basic_block(current_block);

        self.return_blocks.insert(key, return_block);
        Ok(return_block)
    }

    /// Generates LLVM IR for a complete object.
    pub fn generate_object(
        &mut self,
        object: &Object,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        // Reset per-function shared blocks for the new entry point
        self.revert_blocks.clear();
        self.return_blocks.clear();
        self.panic_blocks.clear();

        // Decide whether to use outlined syscall functions based on call-site counts.
        // The function body overhead is only worth paying when there are enough call sites.
        let syscall_counts = object.count_syscall_sites();
        const CALLVALUE_OUTLINE_THRESHOLD: usize = 3;
        const CALLER_OUTLINE_THRESHOLD: usize = 3;
        self.use_outlined_callvalue = syscall_counts.callvalue >= CALLVALUE_OUTLINE_THRESHOLD;
        // Calldataload outlining is disabled: function call overhead (register saves, indirect
        // jump) outweighs the alloca+load savings. Measured as net-negative on OZ ERC20 (+717 bytes).
        self.use_outlined_calldataload = false;
        self.use_outlined_caller = syscall_counts.caller >= CALLER_OUTLINE_THRESHOLD;

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
            self.function_param_types.insert(
                func_id.0,
                function.params.iter().map(|(_, ty)| *ty).collect(),
            );
        }

        // Set LLVM inline attributes based on our custom heuristics.
        // This guides LLVM's inliner for functions that survived our IR-level inlining.
        for function in object.functions.values() {
            self.set_inline_attributes(function, context);
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
                self.inline_decisions.clone(),
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

    /// Sets LLVM inline attributes on a declared function based on our custom heuristics.
    ///
    /// This provides guidance to LLVM's inliner for functions that survived our IR-level
    /// inlining pass. We use AlwaysInline for functions we know should be inlined, and
    /// NoInline only for large functions or those called from many sites where inlining
    /// would cause significant code bloat. For other functions, we let LLVM decide using
    /// its own heuristics (it already has MinSize/OptimizeForSize from set_default_attributes).
    fn set_inline_attributes(&self, function: &Function, context: &PolkaVMContext<'ctx>) {
        let llvm_func = match context.get_function(&function.name, true) {
            Some(func_ref) => func_ref.borrow().declaration().value,
            None => return,
        };

        let llvm_ctx = context.llvm();

        // Check if our IR-level inliner made a decision for this function
        let ir_decision = self.inline_decisions.get(&function.id.0).copied();

        let attr = match ir_decision {
            // Our heuristics say always inline - trust them (they have domain knowledge).
            // These are functions that could not be inlined at IR level (e.g., due to
            // Leave statements inside For loops) but should still be inlined by LLVM.
            Some(crate::InlineDecision::AlwaysInline) => {
                Some(revive_llvm_context::PolkaVMAttribute::AlwaysInline)
            }
            // NeverInline: only enforce NoInline for large functions to prevent bloat.
            // Small NeverInline functions (e.g., dead functions with call_count=0) can
            // be left to LLVM's judgment.
            Some(crate::InlineDecision::NeverInline) => {
                if function.size_estimate >= LARGE_FUNCTION_NOINLINE_THRESHOLD {
                    Some(revive_llvm_context::PolkaVMAttribute::NoInline)
                } else {
                    None
                }
            }
            // CostBenefit: tune inlining based on call count and function size.
            // Small functions called exactly 2 times get AlwaysInline because inlining
            // enables interprocedural optimizations (range proof propagation, constant
            // folding through arguments) that eliminate more code than the duplication
            // adds. Larger functions called 2 times are left to LLVM's judgment (which
            // respects MinSize). Functions with 3+ call sites get NoInline to prevent
            // code bloat from excessive duplication.
            Some(crate::InlineDecision::CostBenefit) => {
                if function.call_count == 2
                    && function.size_estimate <= COST_BENEFIT_INLINE_SIZE_LIMIT
                {
                    Some(revive_llvm_context::PolkaVMAttribute::AlwaysInline)
                } else if function.call_count >= 3 {
                    Some(revive_llvm_context::PolkaVMAttribute::NoInline)
                } else {
                    None
                }
            }
            // No decision (function not analyzed): only hint for very small functions
            None => {
                if function.size_estimate <= SMALL_FUNCTION_ALWAYSINLINE_THRESHOLD {
                    Some(revive_llvm_context::PolkaVMAttribute::AlwaysInline)
                } else {
                    None
                }
            }
        };

        if let Some(attr) = attr {
            llvm_func.add_attribute(
                inkwell::attributes::AttributeLoc::Function,
                llvm_ctx.create_enum_attribute(attr as u32, 0),
            );
        }
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

        // Reset shared blocks for this function scope
        let saved_revert_blocks = std::mem::take(&mut self.revert_blocks);
        let saved_return_blocks = std::mem::take(&mut self.return_blocks);
        let saved_panic_blocks = std::mem::take(&mut self.panic_blocks);

        // Save the current values map and start fresh for this function
        // Each function has its own SSA namespace
        let saved_values = std::mem::take(&mut self.values);

        context.set_current_function(&function.name, None, true)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        // Set up parameters. For narrowed parameters, zero-extend back to word type.
        // This creates an implicit range proof: LLVM knows the value fits in the
        // narrower type, which eliminates overflow checks downstream.
        for (index, (param_id, param_ty)) in function.params.iter().enumerate() {
            let param_value = context.current_function().borrow().get_nth_param(index);
            let stored_value = match param_ty {
                Type::Int(width) if *width < BitWidth::I256 => {
                    let narrow_val = param_value.into_int_value();
                    context
                        .builder()
                        .build_int_z_extend(
                            narrow_val,
                            context.word_type(),
                            &format!("param_{}_extend", index),
                        )
                        .map_err(|e| anyhow::anyhow!("LLVM error: {e}"))?
                        .as_basic_value_enum()
                }
                _ => param_value,
            };
            self.set_value(*param_id, stored_value);
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

        // Restore the saved values map, shared blocks, and caches from before this function
        self.values = saved_values;
        self.revert_blocks = saved_revert_blocks;
        self.return_blocks = saved_return_blocks;
        self.panic_blocks = saved_panic_blocks;

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
            Statement::Continue { .. } => "Continue",
            Statement::Break { .. } => "Break",
            Statement::Stop => "Stop",
            Statement::Invalid => "Invalid",
            Statement::PanicRevert { .. } => "PanicRevert",
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
                if bindings.len() == 1 {
                    let binding_id = bindings[0];
                    let llvm_value =
                        self.try_narrow_let_binding(context, llvm_value, binding_id)?;
                    self.set_value(binding_id, llvm_value);
                } else {
                    // Tuple unpacking - extract each element
                    let struct_val = llvm_value.into_struct_value();
                    for (index, binding) in bindings.iter().enumerate() {
                        let field = context
                            .builder()
                            .build_extract_value(struct_val, index as u32, &format!("{}", index))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        self.set_value(*binding, field);
                    }
                }
            }

            Statement::MStore {
                offset,
                value,
                region,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let value_val = self.translate_value(value)?.into_int_value();
                // MStore requires 256-bit value
                let value_val = self.ensure_word_type(context, value_val, "mstore_val")?;

                // Free memory pointer slot (offset 0x40) is never exposed to external
                // code. Store just the low 32 bits as a native i32 store, avoiding
                // the 32-byte byte-swap overhead of store_heap_word.
                // FMP is bounded by the heap size (~17 bits), so i32 is sufficient.
                let is_fmp_store = matches!(region, MemoryRegion::FreePointerSlot)
                    || Self::try_extract_const_u64(offset_val) == Some(0x40);
                if is_fmp_store {
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "fmp_store_offset")?;
                    // Truncate FMP value to i32 (safe: FMP is always < heap_size < 2^31)
                    let xlen_type = context.xlen_type();
                    let value_i32 =
                        if value_val.get_type().get_bit_width() > xlen_type.get_bit_width() {
                            context
                                .builder()
                                .build_int_truncate(value_val, xlen_type, "fmp_val_trunc")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        } else {
                            value_val
                        };
                    // Direct 4-byte store via unchecked heap GEP. The FMP slot
                    // (offset 0x40) is always within the statically pre-allocated
                    // scratch area, so no sbrk is needed.
                    let pointer = context
                        .build_heap_gep_unchecked(offset_xlen)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context
                        .builder()
                        .build_store(pointer.value, value_i32)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .set_alignment(4)
                        .expect("Alignment is valid");
                } else if self.can_use_native_memory(offset_val) {
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "mstore_offset_xlen")?;
                    revive_llvm_context::polkavm_evm_memory::store_native(
                        context,
                        offset_xlen,
                        value_val,
                    )?;
                } else {
                    let offset_val = self.narrow_offset_for_pointer(
                        context,
                        offset_val,
                        offset.id,
                        "mstore_offset_narrow",
                    )?;
                    revive_llvm_context::polkavm_evm_memory::store(context, offset_val, value_val)?;
                }
            }

            Statement::MStore8 {
                offset,
                value,
                region: _,
            } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "mstore8_offset_narrow",
                )?;
                let value_val = self.translate_value(value)?.into_int_value();
                // MStore8 expects word-sized value (takes low byte)
                let value_val = self.ensure_word_type(context, value_val, "mstore8_val")?;
                revive_llvm_context::polkavm_evm_memory::store_byte(
                    context, offset_val, value_val,
                )?;
            }

            Statement::MCopy { dest, src, length } => {
                let dest_val = self.translate_value(dest)?.into_int_value();
                let dest_val = self.narrow_offset_for_pointer(
                    context,
                    dest_val,
                    dest.id,
                    "mcopy_dest_narrow",
                )?;
                let src_val = self.translate_value(src)?.into_int_value();
                let src_val =
                    self.narrow_offset_for_pointer(context, src_val, src.id, "mcopy_src_narrow")?;
                let len_val = self.translate_value(length)?.into_int_value();
                let len_val = self.narrow_offset_for_pointer(
                    context,
                    len_val,
                    length.id,
                    "mcopy_length_narrow",
                )?;

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
                // Compare at native width - no need to extend to word type
                // since comparing != 0 works correctly at any integer width
                let cond_zero = cond_val.get_type().const_zero();
                let cond_bool = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::NE, cond_val, cond_zero, "cond_bool")
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
                // Use scrutinee at its native width - case constants will match
                let scrut_type = scrut_val.get_type();
                let join_block = context.append_basic_block("switch_join");

                // Create case blocks with constants matching scrutinee width
                let mut case_blocks = Vec::new();
                for (idx, case) in cases.iter().enumerate() {
                    let case_block = context.append_basic_block(&format!("switch_case_{}", idx));
                    let case_u64 = case
                        .value
                        .to_u64()
                        .unwrap_or_else(|| panic!("Case value too large"));
                    let case_val = scrut_type.const_int(case_u64, false);
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
                post_input_vars,
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
                // Landing block merges body-end and continue paths before the post
                let continue_landing = context.append_basic_block("for_continue_landing");
                let post_block = context.append_basic_block("for_post");
                let join_block = context.append_basic_block("for_join");

                context.build_unconditional_branch(cond_block);
                context.set_basic_block(cond_block);

                // Create phi nodes for loop-carried variables at the condition block
                let mut loop_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let mut loop_phi_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
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
                    loop_phi_values.push(phi.as_basic_value());
                    loop_phis.push(phi);
                }

                // Generate condition statements (these may use loop_vars)
                for stmt in condition_stmts {
                    self.generate_statement(stmt, context)?;
                }

                // Evaluate condition
                let cond_val = self.generate_expr(condition, context)?.into_int_value();
                // Compare at native width - no need to extend to word type
                let cond_zero = cond_val.get_type().const_zero();
                let cond_bool = context
                    .builder()
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        cond_val,
                        cond_zero,
                        "for_cond_bool",
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                // Capture the actual block where the condition was evaluated.
                // This may differ from cond_block if the condition expression or
                // condition_stmts created new basic blocks (e.g., shift_right
                // creates overflow/non_overflow/join blocks internally).
                let cond_eval_block = context.basic_block();

                // Create phi nodes at the join block to merge values from
                // the condition-false exit and any break sites.
                context.set_basic_block(join_block);
                let mut join_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let has_loop_vars = !loop_vars.is_empty();
                if has_loop_vars {
                    for i in 0..loop_vars.len() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("join_phi_{}", i))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        join_phis.push(phi);
                    }
                }

                // The condition-false path: loop exits normally, values are the
                // current loop phi values. Branch from the block where the condition
                // was actually evaluated (which may not be cond_block if the condition
                // expression created internal basic blocks).
                context.set_basic_block(cond_eval_block);
                context.build_conditional_branch(cond_bool, body_block, join_block)?;
                if has_loop_vars {
                    for (i, phi) in join_phis.iter().enumerate() {
                        phi.add_incoming(&[(&loop_phi_values[i], cond_eval_block)]);
                    }
                }

                // Create phi nodes at the continue_landing block.
                // These merge body-end values with continue-site values.
                context.set_basic_block(continue_landing);
                let mut landing_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let has_body_yields = !body.yields.is_empty();
                if has_body_yields {
                    for i in 0..body.yields.len() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("continue_landing_{}", i))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        landing_phis.push(phi);
                    }
                }
                context.build_unconditional_branch(post_block);

                // Push loop info for break/continue.
                // continue_block points to continue_landing so continue contributes to the phis.
                context.push_loop(body_block, continue_landing, join_block);

                // Push the post phis info so Continue handler can contribute values
                self.for_loop_post_phis.push(ForLoopPostPhis {
                    phis: landing_phis.clone(),
                    loop_var_phi_values: loop_phi_values.clone(),
                });

                // Push break phis info so Break handler can contribute values
                self.for_loop_break_phis.push(ForLoopBreakPhis {
                    phis: join_phis.clone(),
                    loop_var_phi_values: loop_phi_values.clone(),
                });

                // Generate the body
                context.set_basic_block(body_block);
                self.generate_region(body, context)?;

                // Compute body yield values BEFORE the branch terminator,
                // since translate_value_as_word may emit zext instructions.
                let body_end_block = context.basic_block();
                let mut body_yield_vals: Vec<inkwell::values::BasicValueEnum<'ctx>> = Vec::new();
                if has_body_yields {
                    for (i, yield_ref) in body.yields.iter().enumerate() {
                        let yield_val = self.translate_value_as_word(
                            yield_ref,
                            context,
                            &format!("body_yield_{}", i),
                        )?;
                        body_yield_vals.push(yield_val.as_basic_value_enum());
                    }
                }

                // Body falls through to continue_landing
                context.build_unconditional_branch(continue_landing);

                // Add body-end values to the landing phis
                for (phi, yield_val) in landing_phis.iter().zip(body_yield_vals.iter()) {
                    phi.add_incoming(&[(yield_val, body_end_block)]);
                }

                // Pop the post and break phis info
                self.for_loop_post_phis.pop();
                self.for_loop_break_phis.pop();

                // Now generate the post block. The landing phi values are the
                // "body output" values that the post should use.
                context.set_basic_block(post_block);

                // Map the post's fresh input ValueIds to the landing phi values.
                // The post region references post_input_vars[i] for loop-carried
                // variables. The landing phis merge body-end and continue-site values,
                // so mapping post_input_vars to phi outputs gives the post correct values
                // regardless of whether the body fell through or hit a continue.
                if has_body_yields {
                    for (i, phi) in landing_phis.iter().enumerate() {
                        if i < post_input_vars.len() {
                            self.set_value(post_input_vars[i], phi.as_basic_value());
                        }
                    }
                }

                self.generate_region(post, context)?;

                // Collect yields from post region and wire up cond-block phi nodes
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

                // Set outputs to the join phi values which merge condition-exit
                // and break-site values, giving the correct final iteration values.
                if has_loop_vars {
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < join_phis.len() {
                            self.set_value(*output_id, join_phis[i].as_basic_value());
                        }
                    }
                } else {
                    // No loop vars, fall back to loop phis
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < loop_phis.len() {
                            self.set_value(*output_id, loop_phis[i].as_basic_value());
                        }
                    }
                }
            }

            Statement::Break { values } => {
                // Contribute the break-point values to the join block phis.
                // The IR annotates each break with the current values of the
                // loop-carried variables at the point of the break.
                if let Some(break_phis) = self.for_loop_break_phis.last() {
                    let current_block = context.basic_block();
                    for (i, phi) in break_phis.phis.iter().enumerate() {
                        let value = if i < values.len() {
                            self.translate_value_as_word(
                                &values[i],
                                context,
                                &format!("break_val_{}", i),
                            )?
                            .as_basic_value_enum()
                        } else {
                            break_phis.loop_var_phi_values[i]
                        };
                        phi.add_incoming(&[(&value, current_block)]);
                    }
                }

                let join_block = context.r#loop().join_block;
                context.build_unconditional_branch(join_block);
                // Create unreachable block for dead code after break
                let unreachable = context.append_basic_block("break_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Continue { values } => {
                // Contribute the continue-point values to the landing phis.
                // The IR annotates each continue with the current values of the
                // loop-carried variables at the point of the continue.
                if let Some(post_phis) = self.for_loop_post_phis.last() {
                    let current_block = context.basic_block();
                    for (i, phi) in post_phis.phis.iter().enumerate() {
                        let value = if i < values.len() {
                            self.translate_value_as_word(
                                &values[i],
                                context,
                                &format!("continue_val_{}", i),
                            )?
                            .as_basic_value_enum()
                        } else {
                            post_phis.loop_var_phi_values[i]
                        };
                        phi.add_incoming(&[(&value, current_block)]);
                    }
                }

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
                        // Single return value - must be word type for the pointer
                        if !return_values.is_empty() {
                            if let Ok(ret_val) = self.translate_value(&return_values[0]) {
                                let ret_val = if ret_val.is_int_value() {
                                    self.ensure_word_type(
                                        context,
                                        ret_val.into_int_value(),
                                        "leave_ret_val",
                                    )?
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
                        for (i, ret_val) in return_values.iter().enumerate() {
                            if let Ok(val) = self.translate_value(ret_val) {
                                let val = if val.is_int_value() {
                                    self.ensure_word_type(
                                        context,
                                        val.into_int_value(),
                                        &format!("leave_ret_val_{}", i),
                                    )?
                                    .as_basic_value_enum()
                                } else {
                                    val
                                };
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
                let length_val = self.translate_value(length)?.into_int_value();

                // Check if this is a `revert(0, K)` pattern where K is a constant.
                // These are very common in Solidity contracts:
                // - revert(0, 0): callvalue checks, ABI decoding, overflow guards (100+ sites)
                // - revert(0, 36): error messages with string data (70+ sites)
                // - revert(0, 4): custom error selectors (20+ sites)
                // - revert(0, 68): errors with two arguments (17+ sites)
                // Each site generates the same exit sequence, so we deduplicate by
                // creating ONE shared block per constant length and branching to it.
                if Self::is_const_zero(offset_val) {
                    if let Some(const_len) = Self::try_extract_const_u64(length_val) {
                        let revert_block = self.get_or_create_revert_block(context, const_len)?;
                        context.build_unconditional_branch(revert_block);
                        let dead_block = context.append_basic_block("revert_dedup_dead");
                        context.set_basic_block(dead_block);
                        return Ok(());
                    }
                }
                let offset_val = self.ensure_word_type(context, offset_val, "revert_offset")?;
                let length_val = self.ensure_word_type(context, length_val, "revert_length")?;
                revive_llvm_context::polkavm_evm_return::revert(context, offset_val, length_val)?;
            }

            Statement::Return { offset, length } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let length_val = self.translate_value(length)?.into_int_value();

                // Deduplicate return(const_offset, const_length) patterns by
                // creating ONE shared block per (offset, length) pair and branching to it.
                if let (Some(const_off), Some(const_len)) = (
                    Self::try_extract_const_u64(offset_val),
                    Self::try_extract_const_u64(length_val),
                ) {
                    let return_block =
                        self.get_or_create_return_block(context, const_off, const_len)?;
                    context.build_unconditional_branch(return_block);
                    let dead_block = context.append_basic_block("return_dedup_dead");
                    context.set_basic_block(dead_block);
                    return Ok(());
                }

                let offset_val = self.ensure_word_type(context, offset_val, "return_offset")?;
                let length_val = self.ensure_word_type(context, length_val, "return_length")?;
                revive_llvm_context::polkavm_evm_return::r#return(context, offset_val, length_val)?;
            }

            Statement::Stop => {
                revive_llvm_context::polkavm_evm_return::stop(context)?;
            }

            Statement::Invalid => {
                revive_llvm_context::polkavm_evm_return::invalid(context)?;
            }

            Statement::PanicRevert { code } => {
                let panic_block = self.get_or_create_panic_block(context, *code)?;
                context.build_unconditional_branch(panic_block);
                let dead_block = context.append_basic_block("panic_dedup_dead");
                context.set_basic_block(dead_block);
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
                let args_offset_val = self.narrow_offset_for_pointer(
                    context,
                    args_offset_val,
                    args_offset.id,
                    "call_args_offset_narrow",
                )?;
                let args_length_val = self.translate_value(args_length)?.into_int_value();
                let args_length_val = self.narrow_offset_for_pointer(
                    context,
                    args_length_val,
                    args_length.id,
                    "call_args_length_narrow",
                )?;
                let ret_offset_val = self.translate_value(ret_offset)?.into_int_value();
                let ret_offset_val = self.narrow_offset_for_pointer(
                    context,
                    ret_offset_val,
                    ret_offset.id,
                    "call_ret_offset_narrow",
                )?;
                let ret_length_val = self.translate_value(ret_length)?.into_int_value();
                let ret_length_val = self.narrow_offset_for_pointer(
                    context,
                    ret_length_val,
                    ret_length.id,
                    "call_ret_length_narrow",
                )?;

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
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "create_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "create_length_narrow",
                )?;
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
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "log_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "log_length_narrow",
                )?;
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
                let dest_val = self.narrow_offset_for_pointer(
                    context,
                    dest_val,
                    dest.id,
                    "codecopy_dest_narrow",
                )?;
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "codecopy_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "codecopy_length_narrow",
                )?;
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
                let dest_val = self.narrow_offset_for_pointer(
                    context,
                    dest_val,
                    dest.id,
                    "returndatacopy_dest_narrow",
                )?;
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "returndatacopy_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "returndatacopy_length_narrow",
                )?;
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
                let dest_val = self.narrow_offset_for_pointer(
                    context,
                    dest_val,
                    dest.id,
                    "datacopy_dest_narrow",
                )?;
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
                let dest_val = self.narrow_offset_for_pointer(
                    context,
                    dest_val,
                    dest.id,
                    "calldatacopy_dest_narrow",
                )?;
                let offset_val = self.translate_value(offset)?.into_int_value();
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "calldatacopy_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "calldatacopy_length_narrow",
                )?;
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

                // Comparisons produce i1 and can operate on narrow types.
                // Bitwise ops (And/Or/Xor) are safe with narrow types because
                // and(zext(a), zext(b)) == zext(and(a,b)) — they can't create
                // bits that weren't in the inputs.
                // All other ops use word type for correct EVM wrapping semantics.
                match op {
                    BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt | BinOp::Eq => {
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "cmp")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    BinOp::And | BinOp::Or | BinOp::Xor => {
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "bitwise")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // For add/sub/mul: when type inference proves the result fits in
                    // i64, do the arithmetic at i64 width (native RISC-V ops).
                    // Otherwise extend to i256 for correct modular semantics.
                    BinOp::Add | BinOp::Sub | BinOp::Mul => {
                        let lhs_inferred = self.inferred_width(lhs.id);
                        let rhs_inferred = self.inferred_width(rhs.id);
                        let result_width = match op {
                            BinOp::Add => {
                                crate::type_inference::widen_by_one(lhs_inferred.max(rhs_inferred))
                            }
                            BinOp::Sub => BitWidth::I256,
                            BinOp::Mul => {
                                crate::type_inference::double_width(lhs_inferred.max(rhs_inferred))
                            }
                            _ => unreachable!(),
                        };

                        if result_width.bits() <= 64 {
                            // Result fits in i64 — use narrow arithmetic.
                            // Extend both operands to at least i64 for headroom.
                            let (lhs_val, rhs_val) =
                                self.ensure_min_width(context, lhs_val, rhs_val, 64, "arith")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        } else {
                            // Result needs > i64 — use i256 for correct wrapping.
                            let lhs_val = self.ensure_word_type(context, lhs_val, "arith_lhs")?;
                            let rhs_val = self.ensure_word_type(context, rhs_val, "arith_rhs")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        }
                    }

                    // For div/mod with narrow operands, use native LLVM ops
                    // instead of expensive 256-bit runtime calls
                    BinOp::Div | BinOp::Mod => {
                        let lhs_width = lhs_val.get_type().get_bit_width();
                        let rhs_width = rhs_val.get_type().get_bit_width();
                        if lhs_width <= 64 && rhs_width <= 64 {
                            let (lhs_val, rhs_val) =
                                self.ensure_same_type(context, lhs_val, rhs_val, "narrow_divmod")?;
                            self.generate_narrow_divmod(*op, lhs_val, rhs_val, context)
                        } else {
                            let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                            let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        }
                    }

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
                        // Return i1 directly - avoid i256 zero-extension.
                        // Downstream uses will extend via ensure_word_type when needed.
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
                        Ok(is_zero.as_basic_value_enum())
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
                if self.use_outlined_calldataload {
                    Ok(revive_llvm_context::polkavm_evm_calldata::load_outlined(
                        context, offset_val,
                    )?)
                } else {
                    Ok(revive_llvm_context::polkavm_evm_calldata::load(
                        context, offset_val,
                    )?)
                }
            }

            Expr::CallValue => {
                if self.use_outlined_callvalue {
                    Ok(revive_llvm_context::polkavm_evm_ether_gas::value_outlined(
                        context,
                    )?)
                } else {
                    Ok(revive_llvm_context::polkavm_evm_ether_gas::value(context)?)
                }
            }

            // caller already returns zext i160 → i256 (after byte-swap).
            Expr::Caller => {
                if self.use_outlined_caller {
                    Ok(
                        revive_llvm_context::polkavm_evm_contract_context::caller_outlined(
                            context,
                        )?,
                    )
                } else {
                    Ok(revive_llvm_context::polkavm_evm_contract_context::caller(
                        context,
                    )?)
                }
            }

            // origin already returns zext i160 → i256 (after byte-swap).
            Expr::Origin => Ok(revive_llvm_context::polkavm_evm_contract_context::origin(
                context,
            )?),

            // calldatasize already returns zext i32 → i256, so LLVM knows
            // it fits in 32 bits. No additional range proof needed.
            Expr::CallDataSize => Ok(revive_llvm_context::polkavm_evm_calldata::size(context)?),

            // codesize: both calldatasize and ext_code::size already return zext i32 → i256.
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

            // gas_price already returns zext i32 → i256.
            Expr::GasPrice => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_price(context)?)
            }

            // extcodesize returns zext i32 → i256.
            Expr::ExtCodeSize { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "extcodesize_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::size(
                    context,
                    Some(addr_val),
                )?)
            }

            // returndatasize already returns zext i32 → i256.
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

            // coinbase already returns zext i160 → i256 (after byte-swap).
            Expr::Coinbase => Ok(revive_llvm_context::polkavm_evm_contract_context::coinbase(
                context,
            )?),

            Expr::Timestamp => {
                let val =
                    revive_llvm_context::polkavm_evm_contract_context::block_timestamp(context)?;
                Self::apply_range_proof(context, val, 64, "timestamp")
            }

            Expr::Number => {
                let val = revive_llvm_context::polkavm_evm_contract_context::block_number(context)?;
                Self::apply_range_proof(context, val, 64, "number")
            }

            // difficulty returns a constant, no range proof needed.
            Expr::Difficulty => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::difficulty(context)?)
            }

            // gas_limit already returns zext i32 → i256.
            Expr::GasLimit => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_limit(context)?)
            }

            Expr::ChainId => {
                let val = revive_llvm_context::polkavm_evm_contract_context::chain_id(context)?;
                Self::apply_range_proof(context, val, 64, "chainid")
            }

            Expr::SelfBalance => Ok(revive_llvm_context::polkavm_evm_ether_gas::self_balance(
                context,
            )?),

            Expr::BaseFee => {
                let val = revive_llvm_context::polkavm_evm_contract_context::basefee(context)?;
                Self::apply_range_proof(context, val, 128, "basefee")
            }

            Expr::BlobHash { .. } | Expr::BlobBaseFee => {
                // Blob opcodes return 0 for now (EIP-4844)
                Ok(context.word_const(0).as_basic_value_enum())
            }

            // gas already returns zext i32 → i256.
            Expr::Gas => Ok(revive_llvm_context::polkavm_evm_ether_gas::gas(context)?),

            // msize already returns zext i32 → i256.
            Expr::MSize => Ok(revive_llvm_context::polkavm_evm_memory::msize(context)?),

            // address already returns zext i160 → i256 (after byte-swap).
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

            Expr::MLoad { offset, region } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                let is_free_pointer = matches!(region, MemoryRegion::FreePointerSlot)
                    || Self::is_free_pointer_load(offset_val);

                // Free memory pointer slot uses a native 4-byte load to match
                // the native i32 stores we emit for FreePointerSlot mstores.
                // This avoids the 32-byte byte-swap overhead of load_heap_word.
                let loaded = if is_free_pointer {
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "fmp_load_offset")?;
                    // Load 4 bytes via unchecked heap GEP, then zero-extend to i256.
                    // FMP slot (0x40) is always within the pre-allocated scratch area.
                    let pointer = context
                        .build_heap_gep_unchecked(offset_xlen)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let i32_val = context
                        .builder()
                        .build_load(context.xlen_type(), pointer.value, "fmp_load_i32")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context
                        .basic_block()
                        .get_last_instruction()
                        .expect("Always exists")
                        .set_alignment(4)
                        .expect("Alignment is valid");
                    let word_val = context
                        .builder()
                        .build_int_z_extend(
                            i32_val.into_int_value(),
                            context.word_type(),
                            "fmp_load_ext",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    word_val.as_basic_value_enum()
                } else if self.can_use_native_memory(offset_val) {
                    let offset_xlen =
                        self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                    revive_llvm_context::polkavm_evm_memory::load_native(context, offset_xlen)?
                } else {
                    let offset_val = self.narrow_offset_for_pointer(
                        context,
                        offset_val,
                        offset.id,
                        "mload_offset_narrow",
                    )?;
                    revive_llvm_context::polkavm_evm_memory::load(context, offset_val)?
                };
                // The free memory pointer (mload(64)) is bounded by the heap size
                // (enforced by sbrk). Use a tight range proof: compute the minimum
                // number of bits needed to represent the heap size, then truncate
                // to that width. This proves to LLVM that fmp + small_offset < 2^32,
                // allowing elimination of ALL downstream overflow checks in
                // safe_truncate_int_to_xlen.
                if is_free_pointer {
                    // The FMP is bounded by the heap size (enforced by sbrk).
                    // Use a tight range proof so LLVM can prove fmp + offset < 2^32,
                    // eliminating overflow checks in safe_truncate_int_to_xlen.
                    let heap_size = context
                        .heap_size()
                        .get_zero_extended_constant()
                        .unwrap_or(131072);
                    let raw_bits = 64 - heap_size.leading_zeros();
                    let range_bits = raw_bits.clamp(8, 31);
                    Self::apply_range_proof(context, loaded, range_bits, "fmp")
                } else {
                    Ok(loaded)
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

                // Match argument types to callee's parameter types.
                // If the callee has narrowed parameters (e.g., I64 instead of I256),
                // cast arguments to match. Handle cases where the argument may be
                // wider (truncate), narrower (zero-extend), or same width (no-op).
                let param_types = self.function_param_types.get(&function.0);
                let mut arg_vals = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let param_ty = param_types.and_then(|pts| pts.get(i));
                    let val = match param_ty {
                        Some(Type::Int(width)) if *width < BitWidth::I256 => {
                            let llvm_val = self.translate_value(arg)?;
                            let int_val = llvm_val.into_int_value();
                            let target_type = context.integer_type(width.bits() as usize);
                            let arg_bits = int_val.get_type().get_bit_width();
                            let target_bits = target_type.get_bit_width();
                            if arg_bits > target_bits {
                                context
                                    .builder()
                                    .build_int_truncate(
                                        int_val,
                                        target_type,
                                        &format!("call_arg_narrow_{}", i),
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else if arg_bits < target_bits {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        int_val,
                                        target_type,
                                        &format!("call_arg_widen_{}", i),
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                int_val.as_basic_value_enum()
                            }
                        }
                        _ => {
                            self.translate_value_as_word(arg, context, &format!("call_arg_{}", i))?
                        }
                    };
                    arg_vals.push(val);
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
                let offset_val = self.narrow_offset_for_pointer(
                    context,
                    offset_val,
                    offset.id,
                    "keccak_offset_narrow",
                )?;
                let length_val = self.translate_value(length)?.into_int_value();
                let length_val = self.narrow_offset_for_pointer(
                    context,
                    length_val,
                    length.id,
                    "keccak_length_narrow",
                )?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3(
                    context, offset_val, length_val,
                )?)
            }

            Expr::Keccak256Pair { word0, word1 } => {
                let word0_val = self.translate_value(word0)?.into_int_value();
                let word0_val = self.ensure_word_type(context, word0_val, "keccak_word0")?;
                let word1_val = self.translate_value(word1)?.into_int_value();
                let word1_val = self.ensure_word_type(context, word1_val, "keccak_word1")?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                    context, word0_val, word1_val,
                )?)
            }

            Expr::Keccak256Single { word0 } => {
                let word0_val = self.translate_value(word0)?.into_int_value();
                let word0_val = self.ensure_word_type(context, word0_val, "keccak_word0")?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3_one_word(
                    context, word0_val,
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

    /// Generates a narrow (64-bit or less) unsigned division or modulo.
    /// Uses native LLVM udiv/urem instead of expensive 256-bit runtime calls.
    /// Handles division by zero (returns 0 per EVM spec).
    fn generate_narrow_divmod(
        &mut self,
        op: BinOp,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let int_type = lhs.get_type();
        let zero = int_type.const_zero();

        // Check for division by zero
        let is_zero = context
            .builder()
            .build_int_compare(inkwell::IntPredicate::EQ, rhs, zero, "divmod_iszero")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let non_zero_block = context.append_basic_block("divmod_nonzero");
        let join_block = context.append_basic_block("divmod_join");
        let current_block = context.basic_block();

        context.build_conditional_branch(is_zero, join_block, non_zero_block)?;

        // Non-zero path: do the division
        context.set_basic_block(non_zero_block);
        let result = match op {
            BinOp::Div => context
                .builder()
                .build_int_unsigned_div(lhs, rhs, "narrow_div")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?,
            BinOp::Mod => context
                .builder()
                .build_int_unsigned_rem(lhs, rhs, "narrow_mod")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?,
            _ => unreachable!(),
        };
        let non_zero_exit = context.basic_block();
        context.build_unconditional_branch(join_block);

        // Join: phi between 0 (zero divisor) and result (non-zero divisor)
        context.set_basic_block(join_block);
        let phi = context
            .builder()
            .build_phi(int_type, "divmod_result")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        phi.add_incoming(&[(&zero, current_block), (&result, non_zero_exit)]);

        Ok(phi.as_basic_value().as_basic_value_enum())
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
                // Return i1 directly - avoid i256 zero-extension.
                // Downstream uses will extend via ensure_word_type when needed.
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::ULT, lhs, rhs, "lt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(cmp.as_basic_value_enum())
            }
            BinOp::Gt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::UGT, lhs, rhs, "gt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(cmp.as_basic_value_enum())
            }
            BinOp::Slt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::SLT, lhs, rhs, "slt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(cmp.as_basic_value_enum())
            }
            BinOp::Sgt => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::SGT, lhs, rhs, "sgt")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(cmp.as_basic_value_enum())
            }
            BinOp::Eq => {
                let cmp = context
                    .builder()
                    .build_int_compare(inkwell::IntPredicate::EQ, lhs, rhs, "eq")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                Ok(cmp.as_basic_value_enum())
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
        Self::new(
            HeapOptResults::default(),
            TypeInference::default(),
            BTreeMap::new(),
        )
    }
}
