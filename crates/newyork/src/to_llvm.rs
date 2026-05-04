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
use num::{ToPrimitive, Zero};
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

/// Mode for native memory operations.
///
/// Controls how MLoad/MStore are lowered to LLVM IR:
/// - `AllNative`: All memory accesses are native-safe; use native runtime functions
/// - `InlineNative`: This specific access is native-safe; use inline native code
///   (avoids needing native runtime function declarations in subobjects)
/// - `InlineByteSwap`: Constant offset needing BE; inline byte-swap without sbrk.
///   This lets LLVM fold constant-value byte-swaps at compile time and exposes
///   store-to-load forwarding opportunities that function calls hide.
/// - `ByteSwap`: Must use byte-swapping via shared runtime function (includes sbrk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeMemoryMode {
    /// All accesses safe. Use native runtime function calls.
    AllNative,
    /// This access is safe but others need byte-swapping. Use inline native code.
    InlineNative,
    /// Constant offset; inline byte-swap with unchecked GEP (no sbrk).
    InlineByteSwap,
    /// Must byte-swap for EVM compatibility (dynamic offsets needing sbrk).
    ByteSwap,
}

/// Functions with size_estimate at or above this threshold get NoInline when appropriate.
/// This prevents code bloat from inlining large function bodies at multiple call sites.
const LARGE_FUNCTION_NOINLINE_THRESHOLD: usize = 50;

/// Functions with size_estimate at or below this threshold get AlwaysInline when no
/// IR-level decision was made. Very small functions benefit from inlining.
const SMALL_FUNCTION_ALWAYSINLINE_THRESHOLD: usize = 8;

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
    /// Function return types: maps IR FunctionId to return types.
    /// Used by call sites to zero-extend narrow return values back to i256.
    function_return_types: BTreeMap<u32, Vec<Type>>,
    /// Return types of the currently generating function.
    /// Set during `generate_function` and used by `Leave` codegen.
    current_return_types: Vec<Type>,
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
    /// Set of ValueIds that are bound to `Expr::CallValue`.
    /// When these are used as If conditions, we emit `__revive_callvalue_nonzero()`
    /// returning i1 instead of the full i256 value + comparison.
    callvalue_value_ids: BTreeSet<u32>,
    /// Set of callvalue ValueIds that are ONLY used in callvalue_check patterns.
    /// These bindings can be skipped entirely during codegen because the outlined
    /// __revive_callvalue_check() function handles reading callvalue internally.
    dead_callvalue_ids: BTreeSet<u32>,
    /// Cache of global constants for i256 storage keys.
    /// Maps the string representation of the constant to the global's pointer value.
    /// This avoids materializing the same 256-bit constant at every storage access site.
    storage_key_globals: BTreeMap<String, inkwell::values::PointerValue<'ctx>>,
    /// Cache of outlined keccak256 slot wrapper functions.
    /// Maps the constant slot hash string to the wrapper `FunctionValue`.
    /// Each wrapper is `noinline (i256) -> i256` calling `__revive_keccak256_two_words(word0, CONST)`.
    keccak256_slot_wrappers: BTreeMap<String, inkwell::values::FunctionValue<'ctx>>,
    /// Outlined `__revive_mapping_sload(i256 key, i256 slot) -> i256` function.
    /// Combines keccak256_pair + sload into one function, eliminating redundant bswaps.
    mapping_sload_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Outlined `__revive_mapping_sstore(i256 key, i256 slot, i256 value)` function.
    /// Combines keccak256_pair + sstore into one function, eliminating redundant bswaps.
    mapping_sstore_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Whether to use outlined mapping_sload function (enough call sites to amortize overhead).
    use_outlined_mapping_sload: bool,
    /// Whether to use outlined mapping_sstore function (enough call sites to amortize overhead).
    use_outlined_mapping_sstore: bool,
    /// Whether the contract uses `msize()`. When false, InlineNative stores
    /// can skip the `ensure_heap_size` watermark update.
    has_msize: bool,
    /// Cache of outlined error string revert functions.
    /// Keyed by number of data words (1, 2, 3, etc.).
    /// Each function is `noreturn (i256 length, i256 word0, ...) -> void`.
    error_string_revert_fns: BTreeMap<usize, inkwell::values::FunctionValue<'ctx>>,
    /// Number of ErrorStringRevert sites per data-word count.
    /// Used to decide: outline (>= 2 sites) vs inline (1 site).
    error_string_revert_counts: BTreeMap<usize, usize>,
    /// Cache of outlined custom error revert functions.
    /// Keyed by num_args. The selector is passed as the first parameter.
    custom_error_revert_fns: BTreeMap<usize, inkwell::values::FunctionValue<'ctx>>,
    /// Number of CustomErrorRevert sites per num_args.
    custom_error_revert_counts: BTreeMap<usize, usize>,
    /// Outlined `__revive_bswap256(i256) -> i256` function.
    /// Wraps llvm.bswap.i256 into a noinline function to share the byte-swap
    /// Cached outlined store_bswap function: void(i32 offset, i256 value).
    /// Uses unchecked GEP + 4× bswap.i64 + store. Avoids inlining the bswap
    /// sequence at every call site for variable values.
    store_bswap_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached store_bswap with bounds check: void(i32 offset, i256 value).
    /// Like store_bswap but adds `offset + 32 <= heap_size` check before unchecked GEP.
    /// Used for dynamic-offset ByteSwap stores in non-msize contracts to avoid sbrk.
    store_bswap_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached load_bswap with bounds check: i256(i32 offset).
    /// Adds `offset + 32 <= heap_size` check before unchecked GEP + bswap load.
    /// Used for dynamic-offset ByteSwap loads in non-msize contracts to avoid sbrk.
    load_bswap_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached exit with bounds check: void(i32 flags, i32 offset, i32 length).
    /// Checks `offset + length <= heap_size` before unchecked GEP + seal_return.
    /// Used for dynamic return/revert in non-msize contracts to avoid sbrk.
    exit_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined callvalue check + revert function.
    /// Checks if callvalue is nonzero and reverts with empty data if so.
    callvalue_check_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined storage load by value: i256(i256 key).
    /// Takes key as i256 value (not pointer), internally bswaps + alloca + syscall.
    /// Eliminates alloca+store at each call site for runtime-computed keys.
    sload_word_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined storage store by value: void(i256 key, i256 value).
    /// Takes key and value as i256 values, internally bswaps + alloca + syscall.
    /// Eliminates alloca+store at each call site for runtime-computed keys/values.
    sstore_word_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined zero store with bounds check: void(i32 offset).
    /// Stores 32 zero bytes at heap+offset. Used for constant-zero MStore
    /// in ByteSwap mode to avoid passing an i256 zero parameter.
    store_zero_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined return word: noreturn void(i32 offset, i256 value).
    /// Combines store_bswap_checked + exit_checked for single-word returns.
    /// Bswap-stores value at offset, then seal_returns 32 bytes.
    return_word_fn: Option<inkwell::values::FunctionValue<'ctx>>,
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
            function_return_types: BTreeMap::new(),
            current_return_types: Vec::new(),
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
            callvalue_value_ids: BTreeSet::new(),
            dead_callvalue_ids: BTreeSet::new(),
            storage_key_globals: BTreeMap::new(),
            keccak256_slot_wrappers: BTreeMap::new(),
            has_msize: false,
            error_string_revert_fns: BTreeMap::new(),
            error_string_revert_counts: BTreeMap::new(),
            custom_error_revert_fns: BTreeMap::new(),
            custom_error_revert_counts: BTreeMap::new(),
            store_bswap_fn: None,
            store_bswap_checked_fn: None,
            load_bswap_checked_fn: None,
            exit_checked_fn: None,
            callvalue_check_fn: None,
            sload_word_fn: None,
            sstore_word_fn: None,
            store_zero_checked_fn: None,
            return_word_fn: None,
            mapping_sload_fn: None,
            mapping_sstore_fn: None,
            use_outlined_mapping_sload: false,
            use_outlined_mapping_sstore: false,
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
            function_return_types: BTreeMap::new(),
            current_return_types: Vec::new(),
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
            callvalue_value_ids: BTreeSet::new(),
            dead_callvalue_ids: BTreeSet::new(),
            storage_key_globals: BTreeMap::new(),
            keccak256_slot_wrappers: BTreeMap::new(),
            has_msize: false,
            error_string_revert_fns: BTreeMap::new(),
            error_string_revert_counts: BTreeMap::new(),
            custom_error_revert_fns: BTreeMap::new(),
            custom_error_revert_counts: BTreeMap::new(),
            store_bswap_fn: None,
            store_bswap_checked_fn: None,
            load_bswap_checked_fn: None,
            exit_checked_fn: None,
            callvalue_check_fn: None,
            sload_word_fn: None,
            sstore_word_fn: None,
            store_zero_checked_fn: None,
            return_word_fn: None,
            mapping_sload_fn: None,
            mapping_sstore_fn: None,
            use_outlined_mapping_sload: false,
            use_outlined_mapping_sstore: false,
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

    /// Determines native memory mode for a specific memory access.
    ///
    /// Returns one of four modes:
    /// - `AllNative`: all accesses are safe, use native runtime function calls
    /// - `InlineNative`: this specific access is native-safe; inline store/load (no bswap)
    /// - `InlineByteSwap`: constant offset needing BE; inline bswap without sbrk
    /// - `ByteSwap`: dynamic offset; call shared runtime function (includes sbrk)
    ///
    /// Uses the LLVM constant value directly (not the IR ValueId) to avoid
    /// ValueId namespace collisions between outer objects and subobjects.
    /// The heap analysis tracks variable-accessed offsets to prevent mode
    /// mismatches when the solc M3 optimizer turns literals into variables.
    fn native_memory_mode(&self, offset_llvm: IntValue<'ctx>) -> NativeMemoryMode {
        if self.heap_opt.all_native() {
            return NativeMemoryMode::AllNative;
        }
        if let Some(static_val) = Self::try_extract_const_u64(offset_llvm) {
            // For any constant offset that the heap analysis identifies as native-safe
            // (word-aligned, not tainted, not escaping), use InlineNative.
            // The heap is statically allocated at 131072 bytes, so any constant offset
            // well within that range can safely use unchecked GEP (no sbrk overhead).
            // This enables LLVM's GVN to do store-to-load forwarding on these accesses,
            // potentially eliminating the heap access entirely.
            if self.heap_opt.can_use_native(static_val) {
                return NativeMemoryMode::InlineNative;
            }
            // The free memory pointer at offset 0x40 is a Solidity-internal convention.
            // Even when 0x40 appears in escaping regions (from revert(0, 0x44)), the FMP
            // value gets overwritten by ABI-encoded error data before the escape.
            // This optimization is ONLY safe when no `return` statement covers 0x40 —
            // inline assembly like `return(0, 96)` would return the native FMP value.
            if static_val == 0x40 && self.heap_opt.fmp_native_safe() {
                return NativeMemoryMode::InlineNative;
            }
            // For constant offsets that need byte-swapping (escaping/tainted), use
            // InlineByteSwap: unchecked GEP + inline bswap. This avoids sbrk overhead
            // and lets LLVM fold constant-value byte-swaps at compile time, merge
            // adjacent stores, and do store-to-load forwarding across the bswap.
            return NativeMemoryMode::InlineByteSwap;
        }
        NativeMemoryMode::ByteSwap
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
    ///
    /// Uses the forward-propagated min_width (what the definition produces).
    /// Backward demand narrowing (effective_width) is not used here because
    /// truncating a wide value at the definition site can break overflow
    /// detection in safe_truncate_int_to_xlen and similar safety checks.
    fn inferred_width(&self, id: ValueId) -> BitWidth {
        self.type_info.get(id).min_width
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

    /// Adjusts a single value to an exact target width: truncates if wider,
    /// zero-extends if narrower, returns unchanged if already the target width.
    fn ensure_exact_width(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        target_bits: u32,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let w = value.get_type().get_bit_width();
        if w == target_bits {
            Ok(value)
        } else if w < target_bits {
            let target_type = context.integer_type(target_bits as usize);
            context
                .builder()
                .build_int_z_extend(value, target_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        } else {
            let target_type = context.integer_type(target_bits as usize);
            context
                .builder()
                .build_int_truncate(value, target_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        }
    }

    /// Tries to narrow comparison operands to a smaller type when both are
    /// provably narrow. For unsigned comparisons and equality, truncating both
    /// operands to their proven width is correct and avoids expensive i256
    /// comparison sequences (16-20 RISC-V instructions vs 1-2 for i64).
    ///
    /// Uses three complementary width sources:
    /// 1. LLVM structural analysis (zext, and-mask, lshr, etc.)
    /// 2. Constant width analysis
    /// 3. Forward-propagated type inference min_width
    fn try_narrow_comparison(
        &self,
        context: &PolkaVMContext<'ctx>,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
        a_id: ValueId,
        b_id: ValueId,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let a_width = a.get_type().get_bit_width();
        let b_width = b.get_type().get_bit_width();

        // Only try to narrow if at least one operand is wider than 64 bits
        if a_width <= 64 && b_width <= 64 {
            return self.ensure_same_type(context, a, b, "cmp");
        }

        // Check provable narrow widths from LLVM IR structure
        let a_proven = Self::provable_narrow_width(a).unwrap_or(a_width);
        let b_proven = Self::provable_narrow_width(b).unwrap_or(b_width);

        // Also check constant widths
        let a_effective = if a.is_const() {
            Self::constant_effective_width(a)
                .unwrap_or(a_proven)
                .min(a_proven)
        } else {
            a_proven
        };
        let b_effective = if b.is_const() {
            Self::constant_effective_width(b)
                .unwrap_or(b_proven)
                .min(b_proven)
        } else {
            b_proven
        };

        // Also check forward-propagated type inference min_width.
        // This catches cases where the LLVM IR structure doesn't reveal narrowness
        // but the newyork IR analysis does (e.g., calldatasize, shift results).
        let a_inferred = self.inferred_width(a_id).bits();
        let b_inferred = self.inferred_width(b_id).bits();
        let a_effective = a_effective.min(a_inferred);
        let b_effective = b_effective.min(b_inferred);

        // Both operands must be provably narrow for truncation to be correct
        let max_needed = a_effective.max(b_effective);

        // Map to standard width (8, 32, 64, 128) for LLVM optimization.
        // i128 on riscv64 uses 2 registers (~4 instructions for compare)
        // vs i256 which uses 4 registers (~8 instructions for compare).
        let target_bits = if max_needed <= 8 {
            8
        } else if max_needed <= 32 {
            32
        } else if max_needed <= 64 {
            64
        } else if max_needed <= 128 {
            128
        } else {
            // One or both operands need >128 bits; fall back to widening
            return self.ensure_same_type(context, a, b, "cmp");
        };

        let target_type = context.integer_type(target_bits as usize);

        // Truncate both operands to the target width
        let a_narrow = if a_width > target_bits {
            context
                .builder()
                .build_int_truncate(a, target_type, "cmp_narrow_a")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else if a_width < target_bits {
            context
                .builder()
                .build_int_z_extend(a, target_type, "cmp_ext_a")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else {
            a
        };

        let b_narrow = if b_width > target_bits {
            context
                .builder()
                .build_int_truncate(b, target_type, "cmp_narrow_b")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else if b_width < target_bits {
            context
                .builder()
                .build_int_z_extend(b, target_type, "cmp_ext_b")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        } else {
            b
        };

        Ok((a_narrow, b_narrow))
    }

    /// Narrows a Let-bound value using two complementary strategies:
    ///
    /// 1. **Structural LLVM proofs** (zext source, AND mask, lshr by constant):
    ///    Sound because the proof is based on the instruction producing the value.
    ///
    /// 2. **Backward demand narrowing** from type inference:
    ///    If ALL use sites only need narrow bits (e.g., memory offsets → I64),
    ///    truncate here. LLVM will fold the truncation back into the producing
    ///    operation, converting entire computation chains to narrow types.
    ///    This is sound because modular arithmetic on the lower N bits produces
    ///    the same lower N bits regardless of input width.
    ///
    /// Narrowing to standard widths (i8, i32, i64) reduces:
    /// - Register spill overhead (i64 is 1/4 the spill code of i256)
    /// - Comparison instruction count (i64 is 1 compare vs 4 for i256)
    /// - Overall code size through compound effects on register pressure
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

        // Strategy 1: Check structural proof from the LLVM IR itself.
        if let Some(proven_width) = Self::provable_narrow_width(int_val) {
            let target_bits = if proven_width <= 8 {
                8 // Clamp i1..i8 to i8 for LLVM type compatibility
            } else if proven_width <= 32 {
                32
            } else if proven_width <= 64 {
                64
            } else if proven_width <= 128 {
                128 // i128 uses 2 registers vs 4 for i256 on riscv64
            } else {
                // > 128 bits — not worth narrowing to non-standard widths
                0 // Fall through to strategy 2
            };

            if target_bits > 0 && target_bits < value_width {
                let narrow_type = context.integer_type(target_bits as usize);
                let truncated = context
                    .builder()
                    .build_int_truncate(int_val, narrow_type, "narrow_let")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                return Ok(truncated.as_basic_value_enum());
            }
        }

        // Strategy 2: Backward demand narrowing from type inference.
        // Uses `use_demand_width` which correctly examines ALL recorded use contexts.
        //
        // If every use site only needs ≤I64 bits (e.g., all are memory offsets),
        // truncate here. LLVM will fold the truncation back into the producing
        // operation, converting entire computation chains to narrow types.
        //
        // Safety: For modular-arithmetic operations (add/sub/mul), truncating the
        // result preserves the lower N bits. Since all use sites only observe the
        // lower N bits (proven by backward analysis), the truncation is sound.
        // For memory offsets, any value > 2^32 is invalid on PolkaVM regardless.
        //
        let constraint = self.type_info.get(binding_id);
        if !constraint.is_signed {
            let demand = self.type_info.use_demand_width(binding_id);
            let target_bits = match demand {
                BitWidth::I1 | BitWidth::I8 => 8,
                BitWidth::I32 => 32,
                BitWidth::I64 => 64,
                BitWidth::I128 => 128,
                _ => return Ok(value), // I160 or I256: not worth narrowing
            };
            if target_bits < value_width {
                let narrow_type = context.integer_type(target_bits as usize);
                let truncated = context
                    .builder()
                    .build_int_truncate(int_val, narrow_type, "demand_narrow")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                return Ok(truncated.as_basic_value_enum());
            }
        }

        Ok(value)
    }

    /// Returns the provable bit width of an LLVM value based on structural analysis.
    ///
    /// Only returns a width when the LLVM IR itself proves the value fits,
    /// regardless of type inference. Patterns detected:
    /// - `zext from narrow_type`: value fits in narrow_type's width
    /// - `and %val, constant_mask`: value fits in mask's bit width
    /// - `trunc to narrow_type`: value fits in narrow_type's width
    /// - `lshr %val, constant_amount`: result fits in (input_width - amount) bits
    fn provable_narrow_width(value: IntValue<'ctx>) -> Option<u32> {
        use inkwell::values::InstructionOpcode;

        let instruction = value.as_instruction_value()?;
        match instruction.get_opcode() {
            InstructionOpcode::ZExt => {
                let operand = instruction.get_operand(0)?.value()?.into_int_value();
                // Use the provable narrow width of the source if tighter than its type
                let type_width = operand.get_type().get_bit_width();
                let proven = Self::provable_narrow_width(operand).unwrap_or(type_width);
                Some(proven.min(type_width))
            }
            InstructionOpcode::And => {
                // and %a, %b → result fits in min(width(a), width(b)) bits
                // AND can only clear bits, so the result is bounded by either operand.
                let op0 = instruction.get_operand(0)?.value()?.into_int_value();
                let op1 = instruction.get_operand(1)?.value()?.into_int_value();
                let w0 = if op0.is_const() {
                    Self::constant_effective_width(op0)
                } else {
                    Self::provable_narrow_width(op0)
                };
                let w1 = if op1.is_const() {
                    Self::constant_effective_width(op1)
                } else {
                    Self::provable_narrow_width(op1)
                };
                match (w0, w1) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            }
            InstructionOpcode::Trunc => {
                let target_width = instruction.get_type().into_int_type().get_bit_width();
                // Also check if the source is provably narrower than the target
                let operand = instruction.get_operand(0)?.value()?.into_int_value();
                let src_narrow = Self::provable_narrow_width(operand)
                    .unwrap_or(operand.get_type().get_bit_width());
                Some(src_narrow.min(target_width))
            }
            InstructionOpcode::LShr => {
                // lshr %val, constant_amount → result has at most (width - amount) bits
                let shift_amount = instruction.get_operand(1)?.value()?.into_int_value();
                if let Some(shift) = Self::try_get_small_constant(shift_amount) {
                    let input_width = instruction
                        .get_operand(0)?
                        .value()?
                        .into_int_value()
                        .get_type()
                        .get_bit_width();
                    if shift < input_width as u64 {
                        Some((input_width as u64 - shift) as u32)
                    } else {
                        Some(1) // shift >= width → result is 0
                    }
                } else {
                    None
                }
            }
            InstructionOpcode::Or => {
                // or %a, %b → result fits in max(width(a), width(b)) bits
                let op0 = instruction.get_operand(0)?.value()?.into_int_value();
                let op1 = instruction.get_operand(1)?.value()?.into_int_value();
                let w0 = Self::provable_narrow_width(op0);
                let w1 = Self::provable_narrow_width(op1);
                match (w0, w1) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                }
            }
            InstructionOpcode::Add => {
                // add %a, %b → result fits in max(width(a), width(b)) + 1 bits
                let op0 = instruction.get_operand(0)?.value()?.into_int_value();
                let op1 = instruction.get_operand(1)?.value()?.into_int_value();
                let w0 = Self::provable_narrow_width(op0)
                    .or_else(|| Self::constant_effective_width(op0));
                let w1 = Self::provable_narrow_width(op1)
                    .or_else(|| Self::constant_effective_width(op1));
                match (w0, w1) {
                    (Some(a), Some(b)) => {
                        let result_width = a.max(b) + 1;
                        if result_width <= 128 {
                            Some(result_width)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            InstructionOpcode::Mul => {
                // mul %a, %b → result fits in width(a) + width(b) bits
                let op0 = instruction.get_operand(0)?.value()?.into_int_value();
                let op1 = instruction.get_operand(1)?.value()?.into_int_value();
                let w0 = Self::provable_narrow_width(op0)
                    .or_else(|| Self::constant_effective_width(op0));
                let w1 = Self::provable_narrow_width(op1)
                    .or_else(|| Self::constant_effective_width(op1));
                match (w0, w1) {
                    (Some(a), Some(b)) => {
                        let result_width = a + b;
                        if result_width <= 128 {
                            Some(result_width)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Extracts a small constant value from an IntValue, handling wide types (i256).
    /// Returns None if the value is not a constant or doesn't fit in u64.
    fn try_get_small_constant(int_val: IntValue<'ctx>) -> Option<u64> {
        if let Some(val) = int_val.get_zero_extended_constant() {
            return Some(val);
        }
        // For wider types, try truncate + roundtrip verification
        let wide_type = int_val.get_type();
        if wide_type.get_bit_width() > 64 && int_val.is_const() {
            let i64_type = wide_type.get_context().i64_type();
            let truncated = int_val.const_truncate(i64_type);
            if let Some(val) = truncated.get_zero_extended_constant() {
                let reconstructed = wide_type.const_int(val, false);
                if reconstructed == int_val {
                    return Some(val);
                }
            }
        }
        None
    }

    /// Returns the effective bit width needed to represent a constant integer value.
    /// For wide types (> 64 bits), checks progressively wider truncation targets.
    fn constant_effective_width(int_val: IntValue<'ctx>) -> Option<u32> {
        // For types <= 64 bits, use the direct API
        if let Some(val) = int_val.get_zero_extended_constant() {
            return Some(if val == 0 {
                1
            } else {
                64 - val.leading_zeros()
            });
        }

        // For wider types (e.g., i256), check if the value fits in standard widths.
        // Test from narrow to wide: first 64, then 160. This enables narrowing
        // for common patterns like address masks (160-bit) and small constants.
        let wide_type = int_val.get_type();
        if wide_type.get_bit_width() > 64 && int_val.is_const() {
            // Check if it fits in 64 bits
            let i64_type = wide_type.get_context().i64_type();
            let truncated_64 = int_val.const_truncate(i64_type);
            if let Some(val) = truncated_64.get_zero_extended_constant() {
                let reconstructed = wide_type.const_int(val, false);
                if reconstructed == int_val {
                    return Some(if val == 0 {
                        1
                    } else {
                        64 - val.leading_zeros()
                    });
                }
            }

            // 160-bit constants (e.g., address masks) are generated as i160 at
            // the literal level, so they will be caught by type width checks
            // rather than needing LLVM const-expr analysis here.
        }

        None
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

        // Already at or below xlen (32 bits) — no narrowing needed.
        if value_width <= 32 {
            return Ok(value);
        }

        let inferred = self.inferred_width(source_id);

        // If forward inference proves value fits in 32 bits (xlen), truncate
        // directly to i32. This eliminates the overflow check entirely in
        // safe_truncate_int_to_xlen (which returns immediately for xlen values).
        if matches!(inferred, BitWidth::I1 | BitWidth::I8 | BitWidth::I32) {
            let i32_type = context.llvm().i32_type();
            return context
                .builder()
                .build_int_truncate(value, i32_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()));
        }

        // If value fits in 64 bits but not 32, truncate to i64.
        // safe_truncate_int_to_xlen will do a cheap i64→i32 check.
        if matches!(inferred, BitWidth::I64) && value_width > 64 {
            let i64_type = context.llvm().i64_type();
            return context
                .builder()
                .build_int_truncate(value, i64_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()));
        }

        Ok(value)
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

        // Use outlined __revive_revert functions (shared across all call sites).
        // For length 0: use zero-arg __revive_revert_0() (no argument overhead).
        // For length > 0: use __revive_revert(xlen_length) with pre-truncated xlen arg.
        if const_length == 0 {
            revive_llvm_context::polkavm_evm_return::revert_empty_outlined(context)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        } else {
            let length_xlen = context.xlen_type().const_int(const_length, false);
            revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        // __revive_revert is noreturn; add explicit unreachable for LLVM.
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

        // Emit: mstore(0, 0x4e487b71...) using inline bswap for constant folding
        let zero_xlen = context.xlen_type().const_int(0, false);
        let panic_selector = context
            .word_const_str_hex("4e487b7100000000000000000000000000000000000000000000000000000000");
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            zero_xlen,
            panic_selector,
        )?;

        // Emit: mstore(4, error_code) using inline bswap for constant folding
        let four_xlen = context.xlen_type().const_int(4, false);
        let code_val = context.word_const(error_code as u64);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context, four_xlen, code_val,
        )?;

        // Branch to the shared revert(0, 0x24) block
        let revert_block = self.get_or_create_revert_block(context, 0x24)?;
        context.build_unconditional_branch(revert_block);

        // Restore insertion point
        context.set_basic_block(current_block);

        self.panic_blocks.insert(error_code, panic_block);
        Ok(panic_block)
    }

    /// Gets or creates an outlined function for Error(string) reverts with a given
    /// number of data words. The function signature is:
    ///   `void @__revive_error_string_revert_N(i256 %length, i256 %word0, ...) noreturn`
    ///
    /// The function body loads the free memory pointer, stores the Error(string) ABI
    /// encoding (selector + offset + length + data words), and calls revert.
    fn get_or_create_error_string_revert_fn(
        &mut self,
        num_words: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(&func) = self.error_string_revert_fns.get(&num_words) {
            return Ok(func);
        }

        let word_type = context.word_type();
        let fn_name = format!("__revive_error_string_revert_{num_words}");

        // Build function type: void(i256 length, i256 word0, ..., i256 wordN-1)
        let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = vec![word_type.into()]; // length
        for _ in 0..num_words {
            param_types.push(word_type.into());
        }
        let fn_type = context.llvm().void_type().fn_type(&param_types, false);

        let func = context.module().add_function(
            &fn_name,
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        // Set noinline + noreturn + minsize attributes
        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        // Save current builder position
        let saved_block = context.basic_block();

        // Build the function body
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        // Get parameters
        let length_param = func.get_nth_param(0).unwrap().into_int_value();
        let word_params: Vec<_> = (0..num_words)
            .map(|i| func.get_nth_param((i + 1) as u32).unwrap().into_int_value())
            .collect();

        // Load free memory pointer: mload(0x40)
        let fmp_offset = context.word_const(0x40);
        let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .into_int_value();

        // mstore(fmp, Error(string) selector)
        let error_selector = context
            .word_const_str_hex("08c379a000000000000000000000000000000000000000000000000000000000");
        revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_selector)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // mstore(fmp + 4, 0x20) — string data offset
        let fmp_plus_4 = context
            .builder()
            .build_int_add(fmp, context.word_const(4), "fmp_4")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_val = context.word_const(0x20);
        revive_llvm_context::polkavm_evm_memory::store(context, fmp_plus_4, offset_val)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // mstore(fmp + 0x24, length)
        let fmp_plus_0x24 = context
            .builder()
            .build_int_add(fmp, context.word_const(0x24), "fmp_24")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        revive_llvm_context::polkavm_evm_memory::store(context, fmp_plus_0x24, length_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // mstore(fmp + 0x44, word0), mstore(fmp + 0x64, word1), ...
        for (i, word_param) in word_params.iter().enumerate() {
            let offset = 0x44 + (i as u64) * 0x20;
            let fmp_plus_offset = context
                .builder()
                .build_int_add(fmp, context.word_const(offset), &format!("fmp_{offset:x}"))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
            revive_llvm_context::polkavm_evm_memory::store(context, fmp_plus_offset, *word_param)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        // revert(fmp, total_length)
        let total_length = 0x44 + (num_words as u64) * 0x20;
        let total_length_val = context.word_const(total_length);
        revive_llvm_context::polkavm_evm_return::revert(context, fmp, total_length_val)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        // Restore builder position
        context.set_basic_block(saved_block);

        self.error_string_revert_fns.insert(num_words, func);
        Ok(func)
    }

    /// Gets or creates an outlined function for custom error reverts with N arguments.
    /// The function signature is:
    ///   `void @__revive_custom_error_revert_N(i256 %selector, i256 %arg0, ...) noreturn`
    ///
    /// The function body stores the selector at scratch\[0\], arguments at scratch\[4\],
    /// scratch\[0x24\], etc., and calls revert(0, 4 + 32*N).
    /// Uses store_bswap_unchecked since scratch space offsets are constant and small.
    fn get_or_create_custom_error_revert_fn(
        &mut self,
        num_args: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(&func) = self.custom_error_revert_fns.get(&num_args) {
            return Ok(func);
        }

        let word_type = context.word_type();
        let fn_name = format!("__revive_custom_error_{num_args}");

        // Build function type: void(i256 selector, i256 arg0, ..., i256 argN-1)
        // Selector is passed as the first parameter
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
            (0..=num_args).map(|_| word_type.into()).collect();
        let fn_type = context.llvm().void_type().fn_type(&param_types, false);

        let func = context.module().add_function(
            &fn_name,
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        // Set noinline + noreturn + minsize attributes
        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        // Save current builder position
        let saved_block = context.basic_block();

        // Build the function body
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        // param 0 = selector, params 1..=num_args = dynamic args
        let selector_param = func.get_nth_param(0).unwrap().into_int_value();
        let arg_params: Vec<_> = (1..=num_args)
            .map(|i| func.get_nth_param(i as u32).unwrap().into_int_value())
            .collect();

        // mstore(0, selector)
        let offset_0 = context.xlen_type().const_int(0, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset_0,
            selector_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // mstore(4, arg0), mstore(0x24, arg1), ...
        for (i, arg_param) in arg_params.iter().enumerate() {
            let byte_offset = 4 + (i as u64) * 0x20;
            let offset_val = context.xlen_type().const_int(byte_offset, false);
            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                context, offset_val, *arg_param,
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        // revert(0, 4 + 32*num_args) - use outlined revert to avoid duplicating exit sequence
        let const_len = 4 + (num_args as u64) * 0x20;
        let length_xlen = context.xlen_type().const_int(const_len, false);
        revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        // Restore builder position
        context.set_basic_block(saved_block);

        self.custom_error_revert_fns.insert(num_args, func);
        Ok(func)
    }

    /// Gets or creates an outlined store_bswap function: void(i32 offset, i256 value).
    /// Uses unchecked heap GEP + 4× bswap.i64 + store. This avoids duplicating
    /// the bswap sequence at every variable-value InlineByteSwap store site.
    fn get_or_create_store_bswap_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.store_bswap_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let fn_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let func = context.module().add_function(
            "__revive_store_bswap",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        let offset_param = func.get_nth_param(0).unwrap().into_int_value();
        let value_param = func.get_nth_param(1).unwrap().into_int_value();

        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset_param,
            value_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_bswap_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates an outlined store_bswap with bounds checking:
    /// void(i32 offset, i256 value).
    /// Checks `offset > (heap_size - 32)` and traps if out of bounds,
    /// then uses unchecked GEP + 4× bswap.i64 + store.
    /// This replaces sbrk-based `__revive_store_heap_word` for non-msize contracts.
    fn get_or_create_store_bswap_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.store_bswap_checked_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let fn_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let func = context.module().add_function(
            "__revive_store_bswap_checked",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        let trap_block = context.llvm().append_basic_block(func, "trap");
        let store_block = context.llvm().append_basic_block(func, "store");

        // Entry: bounds check
        context.set_basic_block(entry_block);
        let offset_param = func.get_nth_param(0).unwrap().into_int_value();
        let value_param = func.get_nth_param(1).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_param,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Trap block
        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        // Store block: unchecked GEP + bswap store
        context.set_basic_block(store_block);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset_param,
            value_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_bswap_checked_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates an outlined zero store with bounds checking:
    /// void(i32 offset).
    /// Stores 32 zero bytes at heap+offset. Used for constant-zero MStore
    /// in ByteSwap mode. Saves passing an i256 zero parameter and avoids the
    /// bswap sequence entirely (bswap(0) = 0).
    fn get_or_create_store_zero_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.store_zero_checked_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let fn_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into()], false);
        let func = context.module().add_function(
            "__revive_store_zero_checked",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        let trap_block = context.llvm().append_basic_block(func, "trap");
        let store_block = context.llvm().append_basic_block(func, "store");

        // Entry: bounds check
        context.set_basic_block(entry_block);
        let offset_param = func.get_nth_param(0).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_param,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Trap block
        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        // Store block: store 32 zero bytes
        context.set_basic_block(store_block);
        let pointer = context.build_heap_gep_unchecked(offset_param)?;
        let word_type = context.word_type();
        let zero = word_type.const_zero();
        context
            .builder()
            .build_store(pointer.value, zero)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_zero_checked_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates an outlined return_word function:
    /// noreturn void(i32 offset, i256 value).
    /// Combines store_bswap_checked + exit_checked for single-word returns:
    /// bounds-checks offset, bswap-stores value at heap+offset, then seal_returns 32 bytes.
    /// Eliminates one function call per site and one redundant bounds check.
    fn get_or_create_return_word_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.return_word_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let fn_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let func = context.module().add_function(
            "__revive_return_word",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        let trap_block = context.llvm().append_basic_block(func, "trap");
        let store_block = context.llvm().append_basic_block(func, "store");

        // Entry: bounds check (offset + 32 must fit in heap)
        context.set_basic_block(entry_block);
        let offset_param = func.get_nth_param(0).unwrap().into_int_value();
        let value_param = func.get_nth_param(1).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_param,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Trap block
        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        // Store block: call store_bswap_checked + seal_return
        // Reuse the existing store_bswap_checked function to avoid duplicating
        // the bswap+bounds-check logic. This keeps return_word's body small.
        context.set_basic_block(store_block);
        let store_fn = self.get_or_create_store_bswap_checked_fn(context)?;
        context
            .builder()
            .build_call(store_fn, &[offset_param.into(), value_param.into()], "")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        // seal_return(0, heap_ptr + offset, 32)
        // store_bswap_checked already verified the offset is within heap bounds,
        // so unchecked GEP is safe here.
        let heap_pointer = context
            .build_heap_gep_unchecked(offset_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, xlen_type, "return_word_ptr")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length = xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        context.build_runtime_call(
            "seal_return",
            &[
                xlen_type.const_zero().into(), // flags = 0
                offset_pointer.into(),
                length.into(),
            ],
        );
        context.build_unreachable();

        context.set_basic_block(saved_block);
        self.return_word_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates an outlined load_bswap with bounds checking:
    /// i256(i32 offset).
    /// Checks `offset > (heap_size - 32)` and traps if out of bounds,
    /// then uses unchecked GEP + 4× bswap.i64 + load.
    /// This replaces sbrk-based `__revive_load_heap_word` for non-msize contracts.
    fn get_or_create_load_bswap_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.load_bswap_checked_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let fn_type = word_type.fn_type(&[xlen_type.into()], false);
        let func = context.module().add_function(
            "__revive_load_bswap_checked",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        let trap_block = context.llvm().append_basic_block(func, "trap");
        let load_block = context.llvm().append_basic_block(func, "load");

        // Entry: bounds check
        context.set_basic_block(entry_block);
        let offset_param = func.get_nth_param(0).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_param,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, load_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Trap block
        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        // Load block: unchecked GEP + bswap load
        context.set_basic_block(load_block);
        let result =
            revive_llvm_context::polkavm_evm_memory::load_bswap_unchecked(context, offset_param)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_return(Some(&result))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.load_bswap_checked_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates an outlined exit with bounds checking:
    /// void(i32 flags, i32 offset, i32 length).
    /// Checks `length > heap_size - offset` (catches both offset > heap_size
    /// and offset + length > heap_size) and traps if out of bounds.
    /// Then uses unchecked GEP + seal_return. This replaces the sbrk-based
    /// `__revive_exit` for dynamic return/revert in non-msize contracts.
    fn get_or_create_exit_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.exit_checked_fn {
            return Ok(func);
        }

        let xlen_type = context.xlen_type();
        let fn_type = context.llvm().void_type().fn_type(
            &[xlen_type.into(), xlen_type.into(), xlen_type.into()],
            false,
        );
        let func = context.module().add_function(
            "__revive_exit_checked",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        let trap_block = context.llvm().append_basic_block(func, "trap");
        let exit_block = context.llvm().append_basic_block(func, "exit");

        // Entry: bounds check
        context.set_basic_block(entry_block);
        let flags_param = func.get_nth_param(0).unwrap().into_int_value();
        let offset_param = func.get_nth_param(1).unwrap().into_int_value();
        let length_param = func.get_nth_param(2).unwrap().into_int_value();

        let heap_size = context.heap_size();
        // Check offset >= heap_size first to avoid wrapping in the subtraction below
        let offset_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                offset_param,
                heap_size,
                "offset_oob",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_ok_block = context.llvm().append_basic_block(func, "offset_ok");
        context
            .builder()
            .build_conditional_branch(offset_oob, trap_block, offset_ok_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // offset < heap_size, so subtraction is safe
        context.set_basic_block(offset_ok_block);
        let remaining = context
            .builder()
            .build_int_sub(heap_size, offset_param, "remaining")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                length_param,
                remaining,
                "exit_oob",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(length_oob, trap_block, exit_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Trap block
        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "exit_trap");
        context.build_unreachable();

        // Exit block: unchecked GEP + seal_return
        context.set_basic_block(exit_block);
        let heap_pointer = context
            .build_heap_gep_unchecked(offset_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, xlen_type, "exit_ptr")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_runtime_call(
            "seal_return",
            &[
                flags_param.into(),
                offset_pointer.into(),
                length_param.into(),
            ],
        );
        context.build_unreachable();

        context.set_basic_block(saved_block);
        self.exit_checked_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates the outlined `__revive_sload_word(i256 key) -> i256` function.
    /// Takes the storage key as an i256 value (not pointer), internally handles
    /// bswap, alloca, and the GET_STORAGE syscall. Eliminates alloca+store at
    /// each call site for runtime-computed keys (e.g. keccak256 mapping results).
    fn get_or_create_sload_word_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.sload_word_fn {
            return Ok(func);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let fn_type = word_type.fn_type(&[word_type.into()], false);
        let func = context.module().add_function(
            "__revive_sload_word",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        let key_param = func.get_nth_param(0).unwrap().into_int_value();

        // Byte-swap key (EVM big-endian -> RISC-V little-endian)
        let key_bswap = context
            .build_byte_swap(key_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Alloca for key and value pointers
        let key_pointer = context.build_alloca_at_entry(word_type, "sload_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sload_value");

        // Store bswapped key
        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Call GET_STORAGE syscall
        let is_transient = xlen_type.const_int(0, false);
        let arguments = [
            is_transient.into(),
            key_pointer.to_int(context).into(),
            value_pointer.to_int(context).into(),
        ];
        context.build_runtime_call("get_storage_or_zero", &arguments);

        // Load result and byte-swap back
        let value = context
            .build_load(value_pointer, "sload_result")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context
            .builder()
            .build_return(Some(&value_bswap))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.sload_word_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates the outlined `__revive_sstore_word(i256 key, i256 value)` function.
    /// Takes key and value as i256 values (not pointers), internally handles
    /// bswap, alloca, and the SET_STORAGE syscall. Eliminates alloca+store at
    /// each call site for runtime-computed keys and values.
    fn get_or_create_sstore_word_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.sstore_word_fn {
            return Ok(func);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let fn_type = context
            .llvm()
            .void_type()
            .fn_type(&[word_type.into(), word_type.into()], false);
        let func = context.module().add_function(
            "__revive_sstore_word",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        let key_param = func.get_nth_param(0).unwrap().into_int_value();
        let value_param = func.get_nth_param(1).unwrap().into_int_value();

        // Byte-swap key and value
        let key_bswap = context
            .build_byte_swap(key_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Alloca for key and value pointers
        let key_pointer = context.build_alloca_at_entry(word_type, "sstore_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sstore_value");

        // Store bswapped key and value
        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Call SET_STORAGE syscall
        let is_transient = xlen_type.const_int(0, false);
        let arguments = [
            is_transient.into(),
            key_pointer.to_int(context).into(),
            value_pointer.to_int(context).into(),
        ];
        context.build_runtime_call("set_storage_or_clear", &arguments);

        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.sstore_word_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates `__revive_mapping_sload(i256 key, i256 slot) -> i256`.
    /// Combines keccak256_pair + sload in a single function, eliminating the
    /// redundant bswap pair between keccak output and sload key input.
    /// Uses heap scratch memory for keccak input (same pattern as keccak256_two_words)
    /// and efficient 4x64-bit bswap to minimize function body size.
    fn get_or_create_mapping_sload_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.mapping_sload_fn {
            return Ok(func);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let fn_type = word_type.fn_type(&[word_type.into(), word_type.into()], false);
        let func = context.module().add_function(
            "__revive_mapping_sload",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        let key_param = func.get_nth_param(0).unwrap().into_int_value();
        let slot_param = func.get_nth_param(1).unwrap().into_int_value();

        // Store bswapped key at heap[0] and slot at heap[32] using efficient 4x64-bit
        // bswap (same pattern as __revive_keccak256_two_words). This avoids stack allocas
        // and GEPs, producing a smaller function body.
        let offset0 = xlen_type.const_int(0, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(context, offset0, key_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset32 = xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context, offset32, slot_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Get heap pointer at offset 0 for keccak input
        let input_pointer = context
            .build_heap_gep_unchecked(offset0)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length = xlen_type.const_int(2 * revive_common::BYTE_LENGTH_WORD as u64, false);

        // Hash output on stack - also reused as storage key pointer
        let hash_output = context.build_alloca_at_entry(word_type, "map_sload_hash");
        let value_pointer = context.build_alloca_at_entry(word_type, "map_sload_value");

        // Call hash_keccak_256(input_ptr, 64, hash_output_ptr)
        context.build_runtime_call(
            "hash_keccak_256",
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                hash_output.to_int(context).into(),
            ],
        );

        // Hash output is in LE byte order (native), same as what get_storage_or_zero
        // expects. No bswap needed! Use hash_output directly as storage key.
        let is_transient = xlen_type.const_int(0, false);
        context.build_runtime_call(
            "get_storage_or_zero",
            &[
                is_transient.into(),
                hash_output.to_int(context).into(),
                value_pointer.to_int(context).into(),
            ],
        );

        // Load result and bswap back to EVM big-endian using efficient 4x64-bit swap
        let value = context
            .build_load(value_pointer, "map_sload_result")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context
            .builder()
            .build_return(Some(&value_bswap))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.mapping_sload_fn = Some(func);
        Ok(func)
    }

    /// Gets or creates `__revive_mapping_sstore(i256 key, i256 slot, i256 value)`.
    /// Combines keccak256_pair + sstore in a single function, eliminating the
    /// redundant bswap pair between keccak output and sstore key input.
    /// Uses heap scratch memory for keccak input (same pattern as keccak256_two_words)
    /// and efficient 4x64-bit bswap to minimize function body size.
    fn get_or_create_mapping_sstore_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.mapping_sstore_fn {
            return Ok(func);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let fn_type = context.llvm().void_type().fn_type(
            &[word_type.into(), word_type.into(), word_type.into()],
            false,
        );
        let func = context.module().add_function(
            "__revive_mapping_sstore",
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        let key_param = func.get_nth_param(0).unwrap().into_int_value();
        let slot_param = func.get_nth_param(1).unwrap().into_int_value();
        let value_param = func.get_nth_param(2).unwrap().into_int_value();

        // Store bswapped key at heap[0] and slot at heap[32] using efficient 4x64-bit swap
        let offset0 = xlen_type.const_int(0, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(context, offset0, key_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset32 = xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context, offset32, slot_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Bswap value and store to stack alloca
        let value_bswap = context
            .build_byte_swap(value_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .into_int_value();
        let value_pointer = context.build_alloca_at_entry(word_type, "map_sstore_value");
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // Get heap pointer at offset 0 for keccak input
        let input_pointer = context
            .build_heap_gep_unchecked(offset0)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length = xlen_type.const_int(2 * revive_common::BYTE_LENGTH_WORD as u64, false);

        // Hash output on stack
        let hash_output = context.build_alloca_at_entry(word_type, "map_sstore_hash");

        // Call hash_keccak_256(input_ptr, 64, hash_output_ptr)
        context.build_runtime_call(
            "hash_keccak_256",
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                hash_output.to_int(context).into(),
            ],
        );

        // Hash output is in LE byte order (native), same as what set_storage_or_clear
        // expects. No bswap needed! Use hash_output directly as storage key.
        let is_transient = xlen_type.const_int(0, false);
        context.build_runtime_call(
            "set_storage_or_clear",
            &[
                is_transient.into(),
                hash_output.to_int(context).into(),
                value_pointer.to_int(context).into(),
            ],
        );

        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.mapping_sstore_fn = Some(func);
        Ok(func)
    }

    /// Checks if a region is a simple `revert(0, 0)` with no other side effects.
    /// The region may contain Let bindings (for intermediate zero literals)
    /// followed by a Revert statement.
    fn is_revert_zero_region(region: &crate::ir::Region) -> bool {
        let mut found_revert = false;
        for stmt in &region.statements {
            match stmt {
                Statement::Let {
                    value: Expr::Literal { ref value, .. },
                    ..
                } => {
                    // Allow bindings of literal zeros (common Yul pattern: let v := 0)
                    if !value.is_zero() {
                        return false;
                    }
                }
                Statement::Let { .. } => {
                    return false;
                }
                Statement::Revert { .. } => {
                    found_revert = true;
                }
                _ => return false,
            }
        }
        found_revert
    }

    /// Gets or creates an outlined callvalue check + revert function:
    /// `void __revive_callvalue_check()` that checks if callvalue is nonzero
    /// and reverts with empty data if so. Returns normally if callvalue is zero.
    fn get_or_create_callvalue_check_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(func) = self.callvalue_check_fn {
            return Ok(func);
        }

        let void_type = context.llvm().void_type();
        let fn_type = void_type.fn_type(&[], false);
        let func = context
            .module()
            .add_function("__revive_callvalue_check", fn_type, None);

        // noinline + minsize: don't inline this, optimize for size
        let noinline_attr = context.llvm().create_enum_attribute(
            inkwell::attributes::Attribute::get_named_enum_kind_id("noinline"),
            0,
        );
        let minsize_attr = context.llvm().create_enum_attribute(
            inkwell::attributes::Attribute::get_named_enum_kind_id("minsize"),
            0,
        );
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        func.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(func, "entry");
        context.set_basic_block(entry_block);

        // Call __revive_callvalue_nonzero() to check if callvalue is nonzero
        let is_nonzero =
            revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                .into_int_value();

        let revert_block = context.llvm().append_basic_block(func, "revert");
        let ok_block = context.llvm().append_basic_block(func, "ok");

        context.build_conditional_branch(is_nonzero, revert_block, ok_block)?;

        // Revert block: call __revive_revert_0()
        context.set_basic_block(revert_block);
        revive_llvm_context::polkavm_evm_return::revert_empty_outlined(context)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        // OK block: just return
        context.set_basic_block(ok_block);
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.callvalue_check_fn = Some(func);
        Ok(func)
    }

    /// Emit an exit (return or revert) using unchecked heap GEP, bypassing sbrk.
    /// For non-msize contracts, the heap data was already written by preceding stores,
    /// so sbrk's bounds checking is redundant. The offset/length must be xlen-typed.
    /// `is_revert` controls the flags parameter (0=return, 1=revert).
    fn emit_exit_unchecked(
        &self,
        context: &mut PolkaVMContext<'ctx>,
        offset_xlen: IntValue<'ctx>,
        length_xlen: IntValue<'ctx>,
        is_revert: bool,
    ) -> Result<()> {
        let heap_pointer = context
            .build_heap_gep_unchecked(offset_xlen)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, context.xlen_type(), "exit_data_ptr")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let flags = context.xlen_type().const_int(u64::from(is_revert), false);
        context.build_runtime_call(
            "seal_return",
            &[flags.into(), offset_pointer.into(), length_xlen.into()],
        );
        // No build_unreachable() here: seal_return never returns at runtime
        // but isn't marked noreturn in LLVM IR. Subsequent dead code after the
        // call is valid LLVM IR and will be pruned during optimization.
        Ok(())
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

        let is_deploy = matches!(
            context.code_type(),
            Some(revive_llvm_context::PolkaVMCodeType::Deploy)
        );

        if !self.has_msize && !is_deploy {
            // Bypass sbrk: use unchecked heap GEP for the return data pointer.
            // Constant offsets are within the 131072-byte static heap, so sbrk
            // bounds checking is unnecessary. This eliminates ~5 basic blocks
            // of sbrk overhead per shared return block.
            let offset_xlen = context.xlen_type().const_int(const_offset, false);
            let length_xlen = context.xlen_type().const_int(const_length, false);
            self.emit_exit_unchecked(context, offset_xlen, length_xlen, false)?;
        } else {
            let offset_val = context.word_const(const_offset);
            let length_val = context.word_const(const_length);
            revive_llvm_context::polkavm_evm_return::r#return(context, offset_val, length_val)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        // Ensure the block has a terminator.
        context.build_unreachable();

        // Restore insertion point
        context.set_basic_block(current_block);

        self.return_blocks.insert(key, return_block);
        Ok(return_block)
    }

    /// Find callvalue ValueIds that are ONLY used as conditions in
    /// `if callvalue() { revert(0,0) }` or If condition patterns.
    /// These can be skipped during codegen because __revive_callvalue_check()
    /// and __revive_callvalue_nonzero() handle reading callvalue internally.
    fn find_dead_callvalue_ids(object: &Object) -> BTreeSet<u32> {
        let mut callvalue_ids = BTreeSet::new();
        let mut used_ids = BTreeSet::new();

        // Pass 1: find all callvalue let bindings
        Self::find_callvalue_bindings(&object.code.statements, &mut callvalue_ids);
        for function in object.functions.values() {
            Self::find_callvalue_bindings(&function.body.statements, &mut callvalue_ids);
        }

        // Pass 2: find non-condition uses of callvalue IDs
        Self::find_value_uses(&object.code.statements, &callvalue_ids, &mut used_ids);
        for function in object.functions.values() {
            Self::find_value_uses(&function.body.statements, &callvalue_ids, &mut used_ids);
        }

        // Dead = callvalue IDs with no non-condition uses
        callvalue_ids.difference(&used_ids).copied().collect()
    }

    fn find_callvalue_bindings(stmts: &[Statement], ids: &mut BTreeSet<u32>) {
        for stmt in stmts {
            if let Statement::Let { bindings, value } = stmt {
                if bindings.len() == 1 && matches!(value, Expr::CallValue) {
                    ids.insert(bindings[0].0);
                }
            }
            Self::for_each_nested_region(stmt, |region_stmts| {
                Self::find_callvalue_bindings(region_stmts, ids);
            });
        }
    }

    /// Find uses of callvalue IDs in non-condition positions.
    /// If conditions are OK (handled by callvalue_nonzero); everything else is "used".
    fn find_value_uses(
        stmts: &[Statement],
        callvalue_ids: &BTreeSet<u32>,
        used: &mut BTreeSet<u32>,
    ) {
        for stmt in stmts {
            match stmt {
                Statement::Let { value, .. } | Statement::Expr(value) => {
                    Self::collect_expr_value_refs(value, callvalue_ids, used);
                }
                Statement::MStore { offset, value, .. }
                | Statement::MStore8 { offset, value, .. } => {
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(value.id.0, callvalue_ids, used);
                }
                Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
                    Self::mark_if_callvalue(key.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(value.id.0, callvalue_ids, used);
                }
                Statement::If {
                    // condition is NOT marked as used - handled by callvalue_nonzero
                    inputs,
                    ..
                } => {
                    for v in inputs {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                }
                Statement::Switch {
                    scrutinee, inputs, ..
                } => {
                    Self::mark_if_callvalue(scrutinee.id.0, callvalue_ids, used);
                    for v in inputs {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                }
                Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::Log {
                    offset,
                    length,
                    topics,
                } => {
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                    for t in topics {
                        Self::mark_if_callvalue(t.id.0, callvalue_ids, used);
                    }
                }
                Statement::ExternalCall {
                    gas,
                    address,
                    value,
                    args_offset,
                    args_length,
                    ret_offset,
                    ret_length,
                    ..
                } => {
                    Self::mark_if_callvalue(gas.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(address.id.0, callvalue_ids, used);
                    if let Some(v) = value {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                    Self::mark_if_callvalue(args_offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(args_length.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(ret_offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(ret_length.id.0, callvalue_ids, used);
                }
                Statement::Create {
                    value,
                    offset,
                    length,
                    salt,
                    ..
                } => {
                    Self::mark_if_callvalue(value.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                    if let Some(s) = salt {
                        Self::mark_if_callvalue(s.id.0, callvalue_ids, used);
                    }
                }
                Statement::CustomErrorRevert { args, .. } => {
                    for a in args {
                        Self::mark_if_callvalue(a.id.0, callvalue_ids, used);
                    }
                }
                Statement::Leave { return_values } => {
                    for v in return_values {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                }
                Statement::Break { values } | Statement::Continue { values } => {
                    for v in values {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                }
                Statement::For {
                    init_values,
                    condition,
                    ..
                } => {
                    for v in init_values {
                        Self::mark_if_callvalue(v.id.0, callvalue_ids, used);
                    }
                    Self::collect_expr_value_refs(condition, callvalue_ids, used);
                }
                Statement::MCopy { dest, src, length } => {
                    Self::mark_if_callvalue(dest.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(src.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::SelfDestruct { address } => {
                    Self::mark_if_callvalue(address.id.0, callvalue_ids, used);
                }
                Statement::CodeCopy {
                    dest,
                    offset,
                    length,
                } => {
                    Self::mark_if_callvalue(dest.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::ExtCodeCopy {
                    address,
                    dest,
                    offset,
                    length,
                } => {
                    Self::mark_if_callvalue(address.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(dest.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::MappingSStore { key, slot, value } => {
                    Self::mark_if_callvalue(key.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(slot.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(value.id.0, callvalue_ids, used);
                }
                _ => {}
            }
            // Recurse into nested regions
            Self::for_each_nested_region(stmt, |region_stmts| {
                Self::find_value_uses(region_stmts, callvalue_ids, used);
            });
        }
    }

    fn mark_if_callvalue(id: u32, callvalue_ids: &BTreeSet<u32>, used: &mut BTreeSet<u32>) {
        if callvalue_ids.contains(&id) {
            used.insert(id);
        }
    }

    fn collect_expr_value_refs(
        expr: &Expr,
        callvalue_ids: &BTreeSet<u32>,
        used: &mut BTreeSet<u32>,
    ) {
        match expr {
            Expr::Var(v) => Self::mark_if_callvalue(v.0, callvalue_ids, used),
            Expr::Binary { lhs, rhs, .. } => {
                Self::mark_if_callvalue(lhs.id.0, callvalue_ids, used);
                Self::mark_if_callvalue(rhs.id.0, callvalue_ids, used);
            }
            Expr::Unary { operand, .. }
            | Expr::Truncate { value: operand, .. }
            | Expr::ZeroExtend { value: operand, .. }
            | Expr::SignExtendTo { value: operand, .. } => {
                Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
            }
            Expr::Call { args, .. } => {
                for a in args {
                    Self::mark_if_callvalue(a.id.0, callvalue_ids, used);
                }
            }
            Expr::Keccak256 { offset, length }
            | Expr::Keccak256Pair {
                word0: offset,
                word1: length,
            }
            | Expr::MappingSLoad {
                key: offset,
                slot: length,
            } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
            }
            Expr::Keccak256Single { word0 } => {
                Self::mark_if_callvalue(word0.id.0, callvalue_ids, used);
            }
            Expr::CallDataLoad { offset } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
            }
            Expr::MLoad { offset, .. } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
            }
            _ => {}
        }
    }

    /// Call a closure for each nested region's statements in a statement.
    fn for_each_nested_region<F: FnMut(&[Statement])>(stmt: &Statement, mut f: F) {
        match stmt {
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                f(&then_region.statements);
                if let Some(r) = else_region {
                    f(&r.statements);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    f(&case.body.statements);
                }
                if let Some(d) = default {
                    f(&d.statements);
                }
            }
            Statement::For {
                condition_stmts,
                body,
                post,
                ..
            } => {
                f(condition_stmts);
                f(&body.statements);
                f(&post.statements);
            }
            Statement::Block(region) => {
                f(&region.statements);
            }
            _ => {}
        }
    }

    /// Counts MappingSLoad and MappingSStore operations separately in an object.
    fn count_mapping_ops(object: &Object) -> (usize, usize) {
        fn count_in_stmts(stmts: &[Statement]) -> (usize, usize) {
            let mut sloads = 0;
            let mut sstores = 0;
            for stmt in stmts {
                match stmt {
                    Statement::Let {
                        value: Expr::MappingSLoad { .. },
                        ..
                    } => sloads += 1,
                    Statement::MappingSStore { .. } => sstores += 1,
                    _ => {}
                }
                // Recurse into nested regions
                match stmt {
                    Statement::If {
                        then_region,
                        else_region,
                        ..
                    } => {
                        let (s, w) = count_in_stmts(&then_region.statements);
                        sloads += s;
                        sstores += w;
                        if let Some(r) = else_region {
                            let (s, w) = count_in_stmts(&r.statements);
                            sloads += s;
                            sstores += w;
                        }
                    }
                    Statement::Switch { cases, default, .. } => {
                        for c in cases {
                            let (s, w) = count_in_stmts(&c.body.statements);
                            sloads += s;
                            sstores += w;
                        }
                        if let Some(d) = default {
                            let (s, w) = count_in_stmts(&d.statements);
                            sloads += s;
                            sstores += w;
                        }
                    }
                    Statement::For {
                        condition_stmts,
                        body,
                        post,
                        ..
                    } => {
                        let (s, w) = count_in_stmts(condition_stmts);
                        sloads += s;
                        sstores += w;
                        let (s, w) = count_in_stmts(&body.statements);
                        sloads += s;
                        sstores += w;
                        let (s, w) = count_in_stmts(&post.statements);
                        sloads += s;
                        sstores += w;
                    }
                    Statement::Block(region) => {
                        let (s, w) = count_in_stmts(&region.statements);
                        sloads += s;
                        sstores += w;
                    }
                    _ => {}
                }
            }
            (sloads, sstores)
        }

        let (mut total_sloads, mut total_sstores) = count_in_stmts(&object.code.statements);
        for func in object.functions.values() {
            let (s, w) = count_in_stmts(&func.body.statements);
            total_sloads += s;
            total_sstores += w;
        }
        (total_sloads, total_sstores)
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
        if self.use_outlined_callvalue {
            self.dead_callvalue_ids = Self::find_dead_callvalue_ids(object);
        }
        // Calldataload outlining: share alloca+syscall overhead across call sites.
        const CALLDATALOAD_OUTLINE_THRESHOLD: usize = 20;
        self.use_outlined_calldataload =
            syscall_counts.calldataload >= CALLDATALOAD_OUTLINE_THRESHOLD;
        self.use_outlined_caller = syscall_counts.caller >= CALLER_OUTLINE_THRESHOLD;
        // Count mapping operations to decide if combined mapping functions are worth it.
        // Each compound function body is ~250 bytes. Each call site saves ~12 bytes
        // (one fewer function call). When ALL keccak256_pair usages are compound,
        // __revive_keccak256_two_words has no callers and LLVM DCE eliminates it
        // (~300 bytes saved), drastically lowering the effective threshold.
        // Use a combined threshold with adaptive lowering based on helper elimination.
        const MAPPING_COMBINED_THRESHOLD: usize = 9;
        let (mapping_sloads, mapping_sstores) = Self::count_mapping_ops(object);
        let combined_mapping_ops = mapping_sloads + mapping_sstores;
        // If no remaining Keccak256Pair nodes exist, the keccak helper function will be
        // dead-code eliminated when compound functions are used, giving extra ~300 bytes
        // of savings. This makes compound profitable at much lower call counts.
        self.use_outlined_mapping_sload =
            mapping_sloads > 0 && combined_mapping_ops >= MAPPING_COMBINED_THRESHOLD;
        self.use_outlined_mapping_sstore =
            mapping_sstores > 0 && combined_mapping_ops >= MAPPING_COMBINED_THRESHOLD;
        self.has_msize = object.has_msize();
        self.error_string_revert_counts = object.count_error_string_reverts();
        self.custom_error_revert_counts = object.count_custom_error_reverts();

        // Determine if this is deploy or runtime code and set the code type
        let is_runtime = object.name.ends_with("_deployed");
        if is_runtime {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Runtime);
        } else {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Deploy);
        }

        // Push a function scope for this object's frontend function declarations.
        // The LLVM context requires a scope to track Yul-name → mangled-name mappings
        // when declaring frontend functions via add_function(..., is_frontend: true).
        context.push_function_scope();

        // First pass: declare all user-defined functions
        for (func_id, function) in &object.functions {
            self.declare_function(function, context)?;
            self.function_names.insert(func_id.0, function.name.clone());
            self.function_param_types.insert(
                func_id.0,
                function.params.iter().map(|(_, ty)| *ty).collect(),
            );
            self.function_return_types
                .insert(func_id.0, function.returns.clone());
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
        context.pop_function_scope();

        // Recursively handle subobjects (inner_object for deployed code)
        // Each subobject gets a fresh codegen instance for values (SSA values are scoped to objects)
        // but shares the generated_functions set to avoid regenerating shared utility functions.
        // Each subobject uses its own scoped TypeInference to avoid ValueId namespace collisions.
        for (i, subobject) in object.subobjects.iter().enumerate() {
            let sub_type_info = self
                .type_info
                .sub_inferences
                .get(i)
                .cloned()
                .unwrap_or_else(|| self.type_info.clone());
            let mut subobject_codegen = LlvmCodegen::new_with_shared_functions(
                self.generated_functions.clone(),
                self.heap_opt.clone(),
                sub_type_info,
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

        // Check if any return types are narrowed from I256
        let has_narrow_returns = function
            .returns
            .iter()
            .any(|ty| matches!(ty, Type::Int(bw) if *bw < BitWidth::I256));

        let function_type = if has_narrow_returns {
            let return_types: Vec<_> = function
                .returns
                .iter()
                .map(|ty| match ty {
                    Type::Int(bw) => context.integer_type(bw.bits() as usize),
                    _ => context.word_type(),
                })
                .collect();
            context.function_type_with_returns(argument_types, &return_types)
        } else {
            context.function_type(argument_types, function.returns.len())
        };

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
            // Functions with 2+ call sites get NoInline to prevent code bloat.
            // Single-call functions are left to LLVM's judgment (it respects MinSize).
            Some(crate::InlineDecision::CostBenefit) => {
                if function.call_count >= 2 {
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
        let saved_callvalue_ids = std::mem::take(&mut self.callvalue_value_ids);
        let saved_return_types =
            std::mem::replace(&mut self.current_return_types, function.returns.clone());

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

        // Store return values to return pointer before going to return block.
        // For narrowed return types, truncate the i256 value to the narrow type.
        match context.current_function().borrow().r#return() {
            revive_llvm_context::PolkaVMFunctionReturn::None => {}
            revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                if !function.return_values.is_empty() {
                    if let Ok(ret_val) = self.get_value(function.return_values[0]) {
                        let ret_val = if ret_val.is_int_value() {
                            let int_val = ret_val.into_int_value();
                            match function.returns.first() {
                                Some(Type::Int(bw)) if *bw < BitWidth::I256 => {
                                    let target = context.integer_type(bw.bits() as usize);
                                    let val_bits = int_val.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if val_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(int_val, target, "ret_narrow")
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else if val_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(int_val, target, "ret_widen")
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else {
                                        int_val.as_basic_value_enum()
                                    }
                                }
                                _ => self
                                    .ensure_word_type(context, int_val, "ret_val")?
                                    .as_basic_value_enum(),
                            }
                        } else {
                            ret_val
                        };
                        context.build_store(pointer, ret_val)?;
                    }
                }
            }
            revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                // Multiple return values - build a struct with possibly narrow fields
                let field_types: Vec<_> = (0..size)
                    .map(|i| match function.returns.get(i) {
                        Some(Type::Int(bw)) if *bw < BitWidth::I256 => context
                            .integer_type(bw.bits() as usize)
                            .as_basic_type_enum(),
                        _ => context.word_type().as_basic_type_enum(),
                    })
                    .collect();
                let struct_type = context.structure_type(&field_types);
                let mut struct_val = struct_type.get_undef();
                for (i, ret_id) in function.return_values.iter().enumerate() {
                    if let Ok(ret_val) = self.get_value(*ret_id) {
                        let ret_val = if ret_val.is_int_value() {
                            let int_val = ret_val.into_int_value();
                            match function.returns.get(i) {
                                Some(Type::Int(bw)) if *bw < BitWidth::I256 => {
                                    let target = context.integer_type(bw.bits() as usize);
                                    let val_bits = int_val.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if val_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(
                                                int_val,
                                                target,
                                                &format!("ret_narrow_{}", i),
                                            )
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else if val_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(
                                                int_val,
                                                target,
                                                &format!("ret_widen_{}", i),
                                            )
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else {
                                        int_val.as_basic_value_enum()
                                    }
                                }
                                _ => self
                                    .ensure_word_type(context, int_val, &format!("ret_val_{}", i))?
                                    .as_basic_value_enum(),
                            }
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
        self.callvalue_value_ids = saved_callvalue_ids;
        self.current_return_types = saved_return_types;
        self.revert_blocks = saved_revert_blocks;
        self.return_blocks = saved_return_blocks;
        self.panic_blocks = saved_panic_blocks;

        Ok(())
    }

    /// Generates LLVM IR for a block.
    fn generate_block(&mut self, block: &Block, context: &mut PolkaVMContext<'ctx>) -> Result<()> {
        self.generate_statement_list(&block.statements, context)
    }

    /// Generates LLVM IR for a region.
    fn generate_region(
        &mut self,
        region: &Region,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        self.generate_statement_list(&region.statements, context)
    }

    /// Generates LLVM IR for a list of statements, with look-ahead for compound patterns.
    ///
    /// Detects `MStore(off, val) [+ Let(vN, Literal(32))] + Return(off, 32)` → `return_word`:
    /// combines a bswap-store and seal_return into a single noreturn function call,
    /// eliminating one function call and one redundant bounds check per site.
    fn generate_statement_list(
        &mut self,
        stmts: &[Statement],
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        let mut i = 0;
        while i < stmts.len() {
            // Look-ahead: MStore + [optional Let(const)] + Return(same_offset, 32)
            if let Some(skip) = self.try_match_return_word(stmts, i, context)? {
                i += skip;
                continue;
            }
            self.generate_statement(&stmts[i], context)?;
            i += 1;
        }
        Ok(())
    }

    /// Tries to match and generate a combined return_word pattern.
    /// Returns `Ok(Some(skip_count))` if matched, `Ok(None)` if no match.
    ///
    /// Patterns:
    /// - `MStore(off, val) + Return(off, 32)` (2 statements)
    /// - `MStore(off, val) + Let(vN, Literal(32)) + Return(off, vN)` (3 statements)
    ///
    /// Requirements: same offset ValueId, length = 32, ByteSwap mode, !msize, !deploy.
    fn try_match_return_word(
        &mut self,
        stmts: &[Statement],
        i: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<Option<usize>> {
        if i >= stmts.len() {
            return Ok(None);
        }

        // First statement must be MStore
        let (store_offset, store_value) = match &stmts[i] {
            Statement::MStore { offset, value, .. } => (offset, value),
            _ => return Ok(None),
        };

        // Check preconditions: non-msize, non-deploy runtime code
        if self.has_msize {
            return Ok(None);
        }
        if matches!(
            context.code_type(),
            Some(revive_llvm_context::PolkaVMCodeType::Deploy)
        ) {
            return Ok(None);
        }

        // Try pattern 1: MStore + Return (adjacent, 2 statements)
        // Try pattern 2: MStore + Let(const 32) + Return (3 statements)
        let (ret_offset, ret_length, skip) = if i + 1 < stmts.len() {
            if let Statement::Return { offset, length } = &stmts[i + 1] {
                (offset, length, 2)
            } else if i + 2 < stmts.len() {
                // Check for Let(vN, Literal(32)) + Return(off, vN)
                if let Statement::Return { offset, length } = &stmts[i + 2] {
                    // The intervening statement must be a Let binding for the length
                    if let Statement::Let {
                        bindings,
                        value: Expr::Literal { .. },
                    } = &stmts[i + 1]
                    {
                        if bindings.len() == 1 && bindings[0] == length.id {
                            (offset, length, 3)
                        } else {
                            return Ok(None);
                        }
                    } else {
                        return Ok(None);
                    }
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        // Same offset ValueId
        if store_offset.id != ret_offset.id {
            return Ok(None);
        }

        // Check: offset must be dynamic (ByteSwap mode) — constant offsets already
        // use the shared return block deduplication which is more efficient.
        let offset_val = self.translate_value(store_offset)?.into_int_value();
        if Self::try_extract_const_u64(offset_val).is_some() {
            return Ok(None);
        }

        // Check: length must be constant 32
        // For pattern 2, check the literal value directly without generating the Let.
        // For pattern 1, translate the length and check.
        if skip == 3 {
            // The Let binds a Literal — extract its value directly
            if let Statement::Let {
                value: Expr::Literal { value, .. },
                ..
            } = &stmts[i + 1]
            {
                if value != &num::BigUint::from(revive_common::BYTE_LENGTH_WORD as u64) {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        } else {
            let length_val = self.translate_value(ret_length)?.into_int_value();
            let const_len = Self::try_extract_const_u64(length_val);
            if const_len != Some(revive_common::BYTE_LENGTH_WORD as u64) {
                return Ok(None);
            }
        }

        // Pattern matched! Emit return_word(offset, value).
        let offset_narrow = self.narrow_offset_for_pointer(
            context,
            offset_val,
            store_offset.id,
            "return_word_offset_narrow",
        )?;
        let offset_xlen = context
            .safe_truncate_int_to_xlen(offset_narrow)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let value_val = self.translate_value(store_value)?.into_int_value();
        let value_val = self.ensure_word_type(context, value_val, "return_word_val")?;

        let func = self.get_or_create_return_word_fn(context)?;
        context
            .builder()
            .build_call(func, &[offset_xlen.into(), value_val.into()], "")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        // return_word is noreturn — add unreachable + dead block for subsequent code
        context.build_unreachable();
        let dead_block = context.append_basic_block("return_word_dead");
        context.set_basic_block(dead_block);

        Ok(Some(skip))
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
            Statement::ErrorStringRevert { .. } => "ErrorStringRevert",
            Statement::CustomErrorRevert { .. } => "CustomErrorRevert",
            Statement::DataCopy { .. } => "DataCopy",
            Statement::MappingSStore { .. } => "MappingSStore",
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
                // Track ValueIds bound to CallValue for boolean optimization
                if bindings.len() == 1 && matches!(value, Expr::CallValue) {
                    self.callvalue_value_ids.insert(bindings[0].0);
                    // Skip emitting callvalue syscalls that are only used in check
                    // patterns. The outlined __revive_callvalue_check() and
                    // __revive_callvalue_nonzero() handle reading callvalue internally.
                    if self.dead_callvalue_ids.contains(&bindings[0].0) {
                        return Ok(());
                    }
                }

                // Compute demand hint for single-binding Lets: if every use site
                // only needs ≤64 bits, tell generate_expr so modular BinOps can
                // operate at i64 directly instead of i256+trunc.
                let demand = if bindings.len() == 1 {
                    let binding_id = bindings[0];
                    let constraint = self.type_info.get(binding_id);
                    if !constraint.is_signed {
                        let dw = self.type_info.use_demand_width(binding_id);
                        match dw {
                            BitWidth::I1 | BitWidth::I8 | BitWidth::I32 | BitWidth::I64 => {
                                Some(dw.bits())
                            }
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let llvm_value = self.generate_expr(value, context, demand)?;
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
                        // If the extracted field is a narrow int (from a function with
                        // narrow return types), zero-extend it back to i256 for the
                        // function body which operates on i256 values.
                        let field = if field.is_int_value() {
                            let int_val = field.into_int_value();
                            if int_val.get_type().get_bit_width() < 256 {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        int_val,
                                        context.word_type(),
                                        &format!("{}_extend", index),
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                int_val.as_basic_value_enum()
                            }
                        } else {
                            field
                        };
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

                match self.native_memory_mode(offset_val) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_val,
                            "mstore_offset_xlen",
                        )?;
                        revive_llvm_context::polkavm_evm_memory::store_native(
                            context,
                            offset_xlen,
                            value_val,
                        )?;
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_val,
                            "mstore_offset_xlen",
                        )?;
                        // Constant offsets use unchecked GEP since the static heap
                        // is 131072 bytes and all native-safe constant offsets are
                        // well within that range. This avoids sbrk function call overhead.
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        // The free memory pointer (offset 0x40) only holds heap offsets
                        // bounded by the static heap size (≤131072). Store as i32 to
                        // eliminate 7/8 of the store width on 32-bit PVM.
                        let is_fmp_store = Self::try_extract_const_u64(offset_val) == Some(0x40)
                            && matches!(region, MemoryRegion::FreePointerSlot);
                        let store_val: inkwell::values::BasicValueEnum = if is_fmp_store {
                            context
                                .builder()
                                .build_int_truncate(
                                    value_val,
                                    context.xlen_type(),
                                    "fmp_store_trunc",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .into()
                        } else {
                            value_val.into()
                        };
                        context
                            .builder()
                            .build_store(pointer.value, store_val)
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
                            .expect("Alignment is valid");
                        // Update msize watermark: InlineNative stores bypass sbrk
                        // which normally tracks the heap size.
                        // Skip when the contract doesn't use msize() to save code.
                        if self.has_msize {
                            let static_off =
                                Self::try_extract_const_u64(offset_val).unwrap_or(0x80);
                            let min_size = context.xlen_type().const_int(
                                static_off + revive_common::BYTE_LENGTH_WORD as u64,
                                false,
                            );
                            context
                                .ensure_heap_size(min_size)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        }
                    }
                    NativeMemoryMode::InlineByteSwap => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_val,
                            "mstore_offset_xlen",
                        )?;
                        if value_val.is_const() {
                            // Constant values: inline bswap so LLVM folds bswap(const) = const
                            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                                context,
                                offset_xlen,
                                value_val,
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        } else {
                            // Variable values: call outlined function to avoid
                            // duplicating 4× bswap.i64 sequence at every site
                            let func = self.get_or_create_store_bswap_fn(context)?;
                            let value_word =
                                self.ensure_word_type(context, value_val, "store_bswap_val")?;
                            context
                                .builder()
                                .build_call(
                                    func,
                                    &[offset_xlen.into(), value_word.into()],
                                    "store_bswap",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        }
                        if self.has_msize {
                            let static_off =
                                Self::try_extract_const_u64(offset_val).unwrap_or(0x80);
                            let min_size = context.xlen_type().const_int(
                                static_off + revive_common::BYTE_LENGTH_WORD as u64,
                                false,
                            );
                            context
                                .ensure_heap_size(min_size)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        }
                    }
                    NativeMemoryMode::ByteSwap => {
                        let offset_val = self.narrow_offset_for_pointer(
                            context,
                            offset_val,
                            offset.id,
                            "mstore_offset_narrow",
                        )?;
                        if !self.has_msize {
                            // Non-msize: lightweight bounds check + unchecked GEP.
                            // Avoids sbrk overhead (5+ BBs, watermark update).
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_val)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            if value_val.is_null() {
                                // Zero store: no bswap needed, no i256 parameter.
                                // bswap(0) = 0, so the bswap inside store_bswap_checked
                                // is wasted. Using a dedicated zero-store function
                                // eliminates passing the i256 zero parameter entirely.
                                let func = self.get_or_create_store_zero_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(func, &[offset_xlen.into()], "")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            } else {
                                let func = self.get_or_create_store_bswap_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(func, &[offset_xlen.into(), value_val.into()], "")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            }
                        } else {
                            // Msize contracts: full sbrk for watermark tracking.
                            revive_llvm_context::polkavm_evm_memory::store(
                                context, offset_val, value_val,
                            )?;
                        }
                    }
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
                let key_arg = self.value_to_storage_key_argument(key, context)?;
                if key_arg.is_register() {
                    // Value-based path: key is in a register, use outlined function
                    // to avoid alloca+store at each call site for key and value.
                    let key_val = key_arg
                        .access(context)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let value_val = self
                        .value_to_argument(value, context)?
                        .access(context)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let sstore_fn = self.get_or_create_sstore_word_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            sstore_fn,
                            &[key_val.into(), value_val.into()],
                            "sstore_word",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                } else {
                    // Pointer-based path: key is a global constant pointer.
                    let value_arg = self.value_to_argument(value, context)?;
                    revive_llvm_context::polkavm_evm_storage::store(context, &key_arg, &value_arg)?;
                }
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
                // Optimization: if `if callvalue() { revert(0, 0) }` with no else/outputs,
                // replace the entire if with a single call to an outlined function that
                // checks callvalue and reverts if nonzero. This eliminates the branch,
                // the then-block, and the join-block at each call site.
                if self.use_outlined_callvalue
                    && self.callvalue_value_ids.contains(&condition.id.0)
                    && else_region.is_none()
                    && outputs.is_empty()
                    && Self::is_revert_zero_region(then_region)
                {
                    let func = self.get_or_create_callvalue_check_fn(context)?;
                    context
                        .builder()
                        .build_call(func, &[], "callvalue_check")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    return Ok(());
                }

                // Optimization: if the condition is a callvalue-bound value and we have
                // the outlined callvalue, use __revive_callvalue_nonzero() which returns
                // i1 directly. This avoids a 256-bit comparison at every call site.
                // In OZ ERC20, this saves ~20 sites × ~15 bytes = ~300 bytes.
                let cond_bool = if self.use_outlined_callvalue
                    && self.callvalue_value_ids.contains(&condition.id.0)
                {
                    revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .into_int_value()
                } else {
                    let cond_val = self.translate_value(condition)?.into_int_value();
                    // Compare at native width - no need to extend to word type
                    // since comparing != 0 works correctly at any integer width
                    let cond_zero = cond_val.get_type().const_zero();
                    context
                        .builder()
                        .build_int_compare(
                            inkwell::IntPredicate::NE,
                            cond_val,
                            cond_zero,
                            "cond_bool",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                };

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
                let mut scrut_val = self.translate_value(scrutinee)?.into_int_value();
                let scrut_width = scrut_val.get_type().get_bit_width();

                // Narrow the scrutinee when provably safe. For dispatch switches
                // like `switch shr(224, calldataload(0))`, the scrutinee is i256
                // but provable_narrow_width detects the lshr→32 bits pattern.
                // Narrowing to i32 turns 8-word i256 comparisons into single-word
                // i32 comparisons, saving ~14 PVM instructions per case.
                if scrut_width > 32 {
                    let all_cases_fit_32 = cases
                        .iter()
                        .all(|c| c.value.to_u64().is_some_and(|v| v <= u32::MAX as u64));
                    if all_cases_fit_32 {
                        let provable =
                            Self::provable_narrow_width(scrut_val).unwrap_or(scrut_width);
                        if provable <= 32 {
                            scrut_val = context
                                .builder()
                                .build_int_truncate(
                                    scrut_val,
                                    context.llvm().i32_type(),
                                    "switch_narrow",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        }
                    }
                }

                let scrut_type = scrut_val.get_type();
                let join_block = context.append_basic_block("switch_join");

                // Create case blocks with constants matching scrutinee width.
                // Case values can be up to 256 bits (e.g. switching on a keccak
                // hash or extcodehash), so build them from BigUint limbs rather
                // than assuming they fit in u64.
                let mut case_blocks = Vec::new();
                for (idx, case) in cases.iter().enumerate() {
                    let case_block = context.append_basic_block(&format!("switch_case_{}", idx));
                    let digits = case.value.to_u64_digits();
                    let case_val = if digits.is_empty() {
                        scrut_type.const_zero()
                    } else {
                        scrut_type.const_int_arbitrary_precision(&digits)
                    };
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

                // Create phi nodes for loop-carried variables at the condition block.
                // All phis must be created first (LLVM requires phis grouped at block top).
                let mut loop_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let mut loop_phi_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, _loop_var) in loop_vars.iter().enumerate() {
                    let phi = context
                        .builder()
                        .build_phi(context.word_type(), &format!("loop_var_{}", i))
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    // Add incoming from entry (initial value)
                    if i < init_llvm_values.len() {
                        phi.add_incoming(&[(&init_llvm_values[i], entry_block)]);
                    }

                    loop_phi_values.push(phi.as_basic_value());
                    loop_phis.push(phi);
                }

                // After all phis are created, apply demand-based narrowing and
                // bind the (possibly truncated) values to the loop variables.
                // Loop phis are i256, but if the non-comparison demand is ≤ i64,
                // truncate the value so body operations use the narrower type.
                // This is safe because:
                // 1. Only unsigned loop vars are narrowed (signed needs sign-extend)
                // 2. non_comparison_demand excludes Comparison uses
                // 3. If ANY non-comparison use needs I256 (Arithmetic, StorageAccess,
                //    ExternalCall, etc.), the demand stays at I256 and no truncation occurs
                // 4. Yield values are zero-extended back to i256 via translate_value_as_word
                for (i, loop_var) in loop_vars.iter().enumerate() {
                    let phi_val = loop_phis[i].as_basic_value();
                    let non_comp = self.type_info.non_comparison_demand(*loop_var);
                    let loop_val = if !self.type_info.get(*loop_var).is_signed
                        && matches!(non_comp, BitWidth::I32 | BitWidth::I64)
                    {
                        let narrow_type = context.integer_type(non_comp.bits() as usize);
                        let truncated = context
                            .builder()
                            .build_int_truncate(
                                phi_val.into_int_value(),
                                narrow_type,
                                &format!("loop_narrow_{}", i),
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        truncated.as_basic_value_enum()
                    } else {
                        phi_val
                    };
                    self.set_value(*loop_var, loop_val);
                }

                // Generate condition statements (these may use loop_vars)
                for stmt in condition_stmts {
                    self.generate_statement(stmt, context)?;
                }

                // Evaluate condition
                let cond_val = self
                    .generate_expr(condition, context, None)?
                    .into_int_value();
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
                // Store return values to return pointer before branching to return block.
                // For narrowed return types, truncate values to match the pointer type.
                match context.current_function().borrow().r#return() {
                    revive_llvm_context::PolkaVMFunctionReturn::None => {}
                    revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                        if !return_values.is_empty() {
                            if let Ok(ret_val) = self.translate_value(&return_values[0]) {
                                let ret_val = if ret_val.is_int_value() {
                                    let int_val = ret_val.into_int_value();
                                    match self.current_return_types.first() {
                                        Some(Type::Int(bw)) if *bw < BitWidth::I256 => {
                                            let target = context.integer_type(bw.bits() as usize);
                                            let val_bits = int_val.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if val_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        int_val,
                                                        target,
                                                        "leave_narrow",
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else if val_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        int_val,
                                                        target,
                                                        "leave_widen",
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else {
                                                int_val.as_basic_value_enum()
                                            }
                                        }
                                        _ => self
                                            .ensure_word_type(context, int_val, "leave_ret_val")?
                                            .as_basic_value_enum(),
                                    }
                                } else {
                                    ret_val
                                };
                                context.build_store(pointer, ret_val)?;
                            }
                        }
                    }
                    revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                        let field_types: Vec<_> = (0..size)
                            .map(|i| match self.current_return_types.get(i) {
                                Some(Type::Int(bw)) if *bw < BitWidth::I256 => context
                                    .integer_type(bw.bits() as usize)
                                    .as_basic_type_enum(),
                                _ => context.word_type().as_basic_type_enum(),
                            })
                            .collect();
                        let struct_type = context.structure_type(&field_types);
                        let mut struct_val = struct_type.get_undef();
                        for (i, ret_val) in return_values.iter().enumerate() {
                            if let Ok(val) = self.translate_value(ret_val) {
                                let val = if val.is_int_value() {
                                    let int_val = val.into_int_value();
                                    match self.current_return_types.get(i) {
                                        Some(Type::Int(bw)) if *bw < BitWidth::I256 => {
                                            let target = context.integer_type(bw.bits() as usize);
                                            let val_bits = int_val.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if val_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        int_val,
                                                        target,
                                                        &format!("leave_narrow_{}", i),
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else if val_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        int_val,
                                                        target,
                                                        &format!("leave_widen_{}", i),
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else {
                                                int_val.as_basic_value_enum()
                                            }
                                        }
                                        _ => self
                                            .ensure_word_type(
                                                context,
                                                int_val,
                                                &format!("leave_ret_val_{}", i),
                                            )?
                                            .as_basic_value_enum(),
                                    }
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
                if !self.has_msize {
                    // Non-msize: call shared __revive_exit_checked (noinline).
                    let offset_xlen = context
                        .safe_truncate_int_to_xlen(offset_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let flags = context.xlen_type().const_int(1, false);
                    let func = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            func,
                            &[flags.into(), offset_xlen.into(), length_xlen.into()],
                            "",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                } else {
                    let offset_val = self.ensure_word_type(context, offset_val, "revert_offset")?;
                    let length_val = self.ensure_word_type(context, length_val, "revert_length")?;
                    revive_llvm_context::polkavm_evm_return::revert(
                        context, offset_val, length_val,
                    )?;
                }
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

                if !self.has_msize
                    && !matches!(
                        context.code_type(),
                        Some(revive_llvm_context::PolkaVMCodeType::Deploy)
                    )
                {
                    // Non-msize runtime: call shared __revive_exit_checked (noinline).
                    let offset_xlen = context
                        .safe_truncate_int_to_xlen(offset_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let flags = context.xlen_type().const_int(0, false);
                    let func = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            func,
                            &[flags.into(), offset_xlen.into(), length_xlen.into()],
                            "",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                } else {
                    let offset_val = self.ensure_word_type(context, offset_val, "return_offset")?;
                    let length_val = self.ensure_word_type(context, length_val, "return_length")?;
                    revive_llvm_context::polkavm_evm_return::r#return(
                        context, offset_val, length_val,
                    )?;
                }
            }

            Statement::Stop => {
                // Stop is equivalent to return(0, 0) - use the shared return block
                let return_block = self.get_or_create_return_block(context, 0, 0)?;
                context.build_unconditional_branch(return_block);
                let dead_block = context.append_basic_block("stop_dedup_dead");
                context.set_basic_block(dead_block);
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

            Statement::ErrorStringRevert { length, data } => {
                let num_words = data.len();
                let count = self
                    .error_string_revert_counts
                    .get(&num_words)
                    .copied()
                    .unwrap_or(0);

                // Only outline when >= 2 sites exist to amortize function body cost
                if count >= 2 {
                    let func = self.get_or_create_error_string_revert_fn(num_words, context)?;

                    let length_val = context.word_const(*length as u64);
                    let mut args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![length_val.into()];
                    for word in data {
                        let word_val = context.word_const_str_hex(&word.to_str_radix(16));
                        args.push(word_val.into());
                    }

                    context
                        .builder()
                        .build_call(func, &args, "error_string_revert")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                } else {
                    // Emit inline: same code as the outlined function body
                    let fmp_offset = context.word_const(0x40);
                    let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .into_int_value();

                    let error_sel = context.word_const_str_hex(
                        "08c379a000000000000000000000000000000000000000000000000000000000",
                    );
                    revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_sel)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    let fmp_4 = context
                        .builder()
                        .build_int_add(fmp, context.word_const(4), "fmp_4")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_4,
                        context.word_const(0x20),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    let fmp_24 = context
                        .builder()
                        .build_int_add(fmp, context.word_const(0x24), "fmp_24")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_24,
                        context.word_const(*length as u64),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    for (i, word) in data.iter().enumerate() {
                        let off = 0x44 + (i as u64) * 0x20;
                        let fmp_off = context
                            .builder()
                            .build_int_add(fmp, context.word_const(off), &format!("fmp_{off:x}"))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        let word_val = context.word_const_str_hex(&word.to_str_radix(16));
                        revive_llvm_context::polkavm_evm_memory::store(context, fmp_off, word_val)
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }

                    let total_len = 0x44 + (num_words as u64) * 0x20;
                    revive_llvm_context::polkavm_evm_return::revert(
                        context,
                        fmp,
                        context.word_const(total_len),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                }

                // Add dead block for subsequent code
                let dead_block = context.append_basic_block("error_string_dead");
                context.set_basic_block(dead_block);
            }

            Statement::CustomErrorRevert { selector, args } => {
                let num_args = args.len();
                let count = self
                    .custom_error_revert_counts
                    .get(&num_args)
                    .copied()
                    .unwrap_or(0);

                if count >= 3 {
                    // Outlined path: call shared function with selector + args
                    let func = self.get_or_create_custom_error_revert_fn(num_args, context)?;

                    let selector_val = context.word_const_str_hex(&selector.to_str_radix(16));
                    let mut call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![selector_val.into()];
                    for arg in args {
                        let arg_val = self.translate_value(arg)?.into_int_value();
                        let arg_val =
                            self.ensure_word_type(context, arg_val, "custom_error_arg")?;
                        call_args.push(arg_val.into());
                    }

                    context
                        .builder()
                        .build_call(func, &call_args, "custom_error_revert")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                } else {
                    // Inline path: emit mstores + revert directly using InlineByteSwap
                    let selector_val = context.word_const_str_hex(&selector.to_str_radix(16));
                    let offset_0 = context.xlen_type().const_int(0, false);
                    revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                        context,
                        offset_0,
                        selector_val,
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    for (i, arg) in args.iter().enumerate() {
                        let arg_val = self.translate_value(arg)?.into_int_value();
                        let arg_val =
                            self.ensure_word_type(context, arg_val, "custom_error_arg")?;
                        let byte_offset = 4 + (i as u64) * 0x20;
                        let offset_val = context.xlen_type().const_int(byte_offset, false);
                        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                            context, offset_val, arg_val,
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }

                    let const_len = 4 + (num_args as u64) * 0x20;
                    let revert_block = self.get_or_create_revert_block(context, const_len)?;
                    context.build_unconditional_branch(revert_block);
                }

                // Add dead block for subsequent code
                let dead_block = context.append_basic_block("custom_error_dead");
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

                {
                    // Use sbrk-based log functions for bounds checking (dynamic
                    // offsets could exceed the heap).
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
                let _ = self.generate_expr(expr, context, None)?;
            }

            Statement::MappingSStore { key, slot, value } => {
                let key_val = self.translate_value(key)?.into_int_value();
                let key_val = self.ensure_word_type(context, key_val, "mapping_sstore_key")?;
                let slot_val = self.translate_value(slot)?.into_int_value();
                let slot_val = self.ensure_word_type(context, slot_val, "mapping_sstore_slot")?;
                let value_val = self.translate_value(value)?.into_int_value();
                let value_val =
                    self.ensure_word_type(context, value_val, "mapping_sstore_value")?;

                if self.use_outlined_mapping_sstore {
                    // Call combined mapping_sstore(key, slot, value)
                    let mapping_sstore_fn = self.get_or_create_mapping_sstore_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            mapping_sstore_fn,
                            &[key_val.into(), slot_val.into(), value_val.into()],
                            "mapping_sstore_call",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                } else {
                    // Decompose: keccak256_pair(key, slot) + sstore_word(hash, value)
                    // Use slot wrapper for large constant slots (same as Keccak256Pair)
                    let hash_val =
                        if slot_val.is_const() && Self::try_extract_const_u64(slot_val).is_none() {
                            let wrapper_fn =
                                self.get_or_create_keccak256_slot_wrapper(slot_val, context)?;
                            let fn_type = context
                                .word_type()
                                .fn_type(&[context.word_type().into()], false);
                            context
                                .builder()
                                .build_indirect_call(
                                    fn_type,
                                    wrapper_fn.as_global_value().as_pointer_value(),
                                    &[key_val.into()],
                                    "keccak256_slot_call",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .try_as_basic_value()
                                .basic()
                                .expect("keccak256 slot wrapper should return a value")
                                .into_int_value()
                        } else {
                            revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                                context, key_val, slot_val,
                            )?
                            .into_int_value()
                        };
                    let sstore_fn = self.get_or_create_sstore_word_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            sstore_fn,
                            &[hash_val.into(), value_val.into()],
                            "mapping_sstore_word",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                }
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
    /// Generates LLVM IR for an expression.
    ///
    /// `demand_bits` is an optional hint: when set, the caller only needs
    /// the low `demand_bits` bits of the result. For modular/bitwise BinOps
    /// (Add, Sub, Mul, And, Or, Xor) this allows generating narrow operations
    /// directly instead of computing at i256 and truncating afterward.
    fn generate_expr(
        &mut self,
        expr: &Expr,
        context: &mut PolkaVMContext<'ctx>,
        demand_bits: Option<u32>,
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
                    BinOp::Lt | BinOp::Gt | BinOp::Eq => {
                        // For unsigned comparisons and equality, try to narrow both
                        // operands to a smaller type if provably safe. This reduces
                        // i256 comparisons (16-20 RISC-V instructions) to i64 (1-2).
                        let (lhs_cmp, rhs_cmp) =
                            self.try_narrow_comparison(context, lhs_val, rhs_val, lhs.id, rhs.id)?;
                        self.generate_binop(*op, lhs_cmp, rhs_cmp, context)
                    }
                    BinOp::Slt | BinOp::Sgt => {
                        // Signed comparisons need sign-extension, not truncation.
                        // Keep the standard widening behavior.
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "cmp")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    BinOp::And | BinOp::Or | BinOp::Xor => {
                        // Demand-based narrowing: bitwise ops on truncated
                        // inputs produce the same low bits as on full inputs.
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 64, "dnbit_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 64, "dnbit_r")?;
                                return self.generate_binop(*op, lhs_val, rhs_val, context);
                            }
                            if db <= 128 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 128, "dnbit128_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 128, "dnbit128_r")?;
                                return self.generate_binop(*op, lhs_val, rhs_val, context);
                            }
                        }
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "bitwise")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // For add/sub/mul: when type inference proves the result fits in
                    // i64, do the arithmetic at i64 width (native RISC-V ops).
                    // Otherwise extend to i256 for correct modular semantics.
                    BinOp::Add | BinOp::Sub | BinOp::Mul => {
                        // Demand-based narrowing: for modular arithmetic, the low N
                        // bits of (a op b) depend only on the low N bits of a and b.
                        // When all consumers only need ≤64 bits, operate at i64.
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 64, "dnarith_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 64, "dnarith_r")?;
                                return self.generate_binop(*op, lhs_val, rhs_val, context);
                            }
                            if db <= 128 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 128, "dnarith128_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 128, "dnarith128_r")?;
                                return self.generate_binop(*op, lhs_val, rhs_val, context);
                            }
                        }

                        let lhs_inferred = self.inferred_width(lhs.id);
                        let rhs_inferred = self.inferred_width(rhs.id);
                        let max_operand = lhs_inferred.max(rhs_inferred);

                        // For i64 tier: use widen_by_one to guarantee no overflow.
                        // add(I32, I32) → I64 guaranteed, sub(I32, I32) → i64 wraps correctly.
                        let result_fits_i64 = match op {
                            BinOp::Add => {
                                crate::type_inference::widen_by_one(max_operand).bits() <= 64
                            }
                            BinOp::Sub => false,
                            BinOp::Mul => {
                                crate::type_inference::double_width(max_operand).bits() <= 64
                            }
                            _ => unreachable!(),
                        };

                        // For i128 tier: if both operands fit in i64, the result of
                        // add/sub fits in i65 (< i128). For mul, if both operands fit
                        // in i64, the product fits in i128.
                        let result_fits_i128 = max_operand.bits() <= 64;

                        if result_fits_i64 {
                            // Result fits in i64 — use narrow arithmetic.
                            let (lhs_val, rhs_val) =
                                self.ensure_min_width(context, lhs_val, rhs_val, 64, "arith")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        } else if result_fits_i128 {
                            // Both operands ≤ I64: result fits in i128 (65 bits for
                            // add/sub, 128 bits for mul). Uses 2 registers on riscv64.
                            let (lhs_val, rhs_val) =
                                self.ensure_min_width(context, lhs_val, rhs_val, 128, "arith128")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        } else {
                            // Result needs > i128 — use i256 for correct wrapping.
                            let lhs_val = self.ensure_word_type(context, lhs_val, "arith_lhs")?;
                            let rhs_val = self.ensure_word_type(context, rhs_val, "arith_rhs")?;
                            self.generate_binop(*op, lhs_val, rhs_val, context)
                        }
                    }

                    // For div/mod with structurally narrow operands (LLVM type ≤ 64 bits),
                    // use native ops instead of expensive 256-bit runtime calls.
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

                    // Shift left: for constant shift amounts where demand ≤ 64,
                    // operate at i64 width. For non-constant or large shifts, use i256.
                    BinOp::Shl => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                if let Some(shift) = Self::try_get_small_constant(lhs_val) {
                                    if shift >= 64 {
                                        // Shift ≥ 64 at i64 width → all low bits are 0
                                        let i64_type = context.llvm().i64_type();
                                        return Ok(i64_type.const_zero().as_basic_value_enum());
                                    }
                                    let rhs_narrow =
                                        self.ensure_exact_width(context, rhs_val, 64, "dnshl_val")?;
                                    let lhs_narrow =
                                        self.ensure_exact_width(context, lhs_val, 64, "dnshl_amt")?;
                                    let result = context
                                        .builder()
                                        .build_left_shift(rhs_narrow, lhs_narrow, "shl_dn")
                                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                                    return Ok(result.as_basic_value_enum());
                                }
                            }
                        }
                        let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                        let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
                    }

                    // Shift right: if the value is provably narrow (≤ 64 bits) AND
                    // the shift amount is a known constant < 64, operate at i64 width.
                    BinOp::Shr => {
                        let rhs_inferred = self.inferred_width(rhs.id);
                        if rhs_inferred.bits() <= 64 {
                            if let Some(shift) = Self::try_get_small_constant(lhs_val) {
                                if shift >= 64 {
                                    let i64_type = context.llvm().i64_type();
                                    return Ok(i64_type.const_zero().as_basic_value_enum());
                                }
                                let rhs_narrow =
                                    self.ensure_exact_width(context, rhs_val, 64, "dnshr_val")?;
                                let lhs_narrow =
                                    self.ensure_exact_width(context, lhs_val, 64, "dnshr_amt")?;
                                let result = context
                                    .builder()
                                    .build_right_shift(rhs_narrow, lhs_narrow, false, "shr_dn")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                                return Ok(result.as_basic_value_enum());
                            }
                        }
                        let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                        let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                        self.generate_binop(*op, lhs_val, rhs_val, context)
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
                        // Demand-based narrowing: the low N bits of NOT(x) depend only
                        // on the low N bits of x. When demand ≤ 64, operate at i64.
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let narrow_val =
                                    self.ensure_exact_width(context, operand_val, 64, "dnnot_op")?;
                                let all_ones = narrow_val.get_type().const_all_ones();
                                let xor_result = context
                                    .builder()
                                    .build_xor(narrow_val, all_ones, "not_narrow")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                                return Ok(xor_result.as_basic_value_enum());
                            }
                        }
                        // Full 256-bit NOT for cases without narrow demand.
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
                // Only apply FMP range proof when the heap analysis confirms 0x40
                // is native-safe (i.e., only used as FMP, not user data in inline asm).
                let is_free_pointer = (matches!(region, MemoryRegion::FreePointerSlot)
                    || Self::is_free_pointer_load(offset_val))
                    && (self.heap_opt.can_use_native(0x40) || self.heap_opt.fmp_native_safe());

                let loaded = match self.native_memory_mode(offset_val) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen =
                            self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                        revive_llvm_context::polkavm_evm_memory::load_native(context, offset_xlen)?
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen =
                            self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                        // Constant offsets use unchecked GEP since the static heap
                        // is 131072 bytes and all native-safe offsets fit easily.
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        if is_free_pointer {
                            // FMP only holds heap offsets bounded by heap size (≤131072).
                            // Load as i32 and zero-extend: eliminates 7/8 of the load
                            // width on 32-bit PVM and makes the range proof redundant.
                            let narrow = context
                                .builder()
                                .build_load(context.xlen_type(), pointer.value, "fmp_load")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .into_int_value();
                            context
                                .builder()
                                .build_int_z_extend(narrow, context.word_type(), "fmp_zext")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .as_basic_value_enum()
                        } else {
                            context
                                .builder()
                                .build_load(context.word_type(), pointer.value, "native_load")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .as_basic_value_enum()
                        }
                    }
                    NativeMemoryMode::InlineByteSwap => {
                        let offset_xlen =
                            self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                        revive_llvm_context::polkavm_evm_memory::load_bswap_unchecked(
                            context,
                            offset_xlen,
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    }
                    NativeMemoryMode::ByteSwap => {
                        let offset_val = self.narrow_offset_for_pointer(
                            context,
                            offset_val,
                            offset.id,
                            "mload_offset_narrow",
                        )?;
                        if !self.has_msize {
                            // Non-msize: lightweight bounds check + unchecked GEP.
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_val)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            let func = self.get_or_create_load_bswap_checked_fn(context)?;
                            context
                                .builder()
                                .build_call(func, &[offset_xlen.into()], "checked_load")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .try_as_basic_value()
                                .basic()
                                .expect("load_bswap_checked should return a value")
                        } else {
                            // Msize contracts: full sbrk for watermark tracking.
                            revive_llvm_context::polkavm_evm_memory::load(context, offset_val)?
                        }
                    }
                };
                // The free memory pointer (mload(64)) is bounded by the heap size
                // (enforced by sbrk). Use a tight range proof: compute the minimum
                // number of bits needed to represent the heap size, then truncate
                // to that width. This proves to LLVM that fmp + small_offset < 2^32,
                // allowing elimination of ALL downstream overflow checks in
                // safe_truncate_int_to_xlen.
                if is_free_pointer {
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
                let key_arg = self.value_to_storage_key_argument(key, context)?;
                if key_arg.is_register() {
                    // Value-based path: key is in a register, use outlined function
                    // to avoid alloca+store at each call site.
                    let key_val = key_arg
                        .access(context)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let sload_fn = self.get_or_create_sload_word_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(sload_fn, &[key_val.into()], "sload_word")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .try_as_basic_value()
                        .basic()
                        .expect("sload_word should return a value");
                    Ok(result)
                } else {
                    // Pointer-based path: key is a global constant pointer.
                    Ok(revive_llvm_context::polkavm_evm_storage::load(
                        context, &key_arg,
                    )?)
                }
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
                let result = result.unwrap_or_else(|| context.word_const(0).as_basic_value_enum());

                // For functions with a single narrow return type, zero-extend back to i256
                // so downstream uses (which operate on i256) work correctly.
                // Multi-return (struct) results are handled at the extract_value site.
                let return_types = self.function_return_types.get(&function.0);
                let result = match return_types {
                    Some(ret_types)
                        if ret_types.len() == 1
                            && matches!(ret_types[0], Type::Int(bw) if bw < BitWidth::I256) =>
                    {
                        if result.is_int_value() {
                            let int_val = result.into_int_value();
                            if int_val.get_type().get_bit_width() < 256 {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        int_val,
                                        context.word_type(),
                                        "call_ret_extend",
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                result
                            }
                        } else {
                            result
                        }
                    }
                    _ => result,
                };

                Ok(result)
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

                // Use outlined slot wrapper when word1 is a large constant
                if word1_val.is_const() && Self::try_extract_const_u64(word1_val).is_none() {
                    let wrapper_fn =
                        self.get_or_create_keccak256_slot_wrapper(word1_val, context)?;
                    let fn_type = context
                        .word_type()
                        .fn_type(&[context.word_type().into()], false);
                    let result = context
                        .builder()
                        .build_indirect_call(
                            fn_type,
                            wrapper_fn.as_global_value().as_pointer_value(),
                            &[word0_val.into()],
                            "keccak256_slot_call",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    Ok(result
                        .try_as_basic_value()
                        .basic()
                        .expect("keccak256 slot wrapper should return a value"))
                } else {
                    Ok(revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                        context, word0_val, word1_val,
                    )?)
                }
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

            Expr::MappingSLoad { key, slot } => {
                let key_val = self.translate_value(key)?.into_int_value();
                let key_val = self.ensure_word_type(context, key_val, "mapping_sload_key")?;
                let slot_val = self.translate_value(slot)?.into_int_value();
                let slot_val = self.ensure_word_type(context, slot_val, "mapping_sload_slot")?;

                if self.use_outlined_mapping_sload {
                    // Call combined mapping_sload(key, slot)
                    let mapping_sload_fn = self.get_or_create_mapping_sload_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(
                            mapping_sload_fn,
                            &[key_val.into(), slot_val.into()],
                            "mapping_sload_call",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .try_as_basic_value()
                        .basic()
                        .expect("mapping_sload should return a value");
                    Ok(result)
                } else {
                    // Decompose: keccak256_pair(key, slot) + sload_word(hash)
                    // Use slot wrapper for large constant slots (same as Keccak256Pair)
                    let hash_val =
                        if slot_val.is_const() && Self::try_extract_const_u64(slot_val).is_none() {
                            let wrapper_fn =
                                self.get_or_create_keccak256_slot_wrapper(slot_val, context)?;
                            let fn_type = context
                                .word_type()
                                .fn_type(&[context.word_type().into()], false);
                            context
                                .builder()
                                .build_indirect_call(
                                    fn_type,
                                    wrapper_fn.as_global_value().as_pointer_value(),
                                    &[key_val.into()],
                                    "keccak256_slot_call",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .try_as_basic_value()
                                .basic()
                                .expect("keccak256 slot wrapper should return a value")
                                .into_int_value()
                        } else {
                            revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                                context, key_val, slot_val,
                            )?
                            .into_int_value()
                        };
                    let sload_fn = self.get_or_create_sload_word_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(sload_fn, &[hash_val.into()], "mapping_sload_word")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .try_as_basic_value()
                        .basic()
                        .expect("sload_word should return a value");
                    Ok(result)
                }
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
            BinOp::Shl => {
                // For constant shift amounts, skip the overflow check and generate
                // direct shl. This eliminates branch + phi overhead for known shifts.
                if let Some(shift) = Self::try_get_small_constant(lhs) {
                    if shift >= 256 {
                        return Ok(rhs.get_type().const_zero().as_basic_value_enum());
                    }
                    let result = context
                        .builder()
                        .build_left_shift(rhs, lhs, "shl_const")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    return Ok(result.as_basic_value_enum());
                }
                Ok(revive_llvm_context::polkavm_evm_bitwise::shift_left(
                    context, lhs, rhs,
                )?)
            }
            BinOp::Shr => {
                // For constant shift amounts, skip the overflow check and generate
                // direct lshr. This produces a clean instruction that
                // provable_narrow_width can detect, enabling downstream narrowing
                // (e.g., shr(224, calldataload(0)) → i32 selector).
                if let Some(shift) = Self::try_get_small_constant(lhs) {
                    if shift >= 256 {
                        return Ok(rhs.get_type().const_zero().as_basic_value_enum());
                    }
                    let result = context
                        .builder()
                        .build_right_shift(rhs, lhs, false, "shr_const")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    return Ok(result.as_basic_value_enum());
                }
                Ok(revive_llvm_context::polkavm_evm_bitwise::shift_right(
                    context, lhs, rhs,
                )?)
            }
            BinOp::Sar => Ok(
                revive_llvm_context::polkavm_evm_bitwise::shift_right_arithmetic(
                    context, lhs, rhs,
                )?,
            ),
            // Comparisons return i1 directly — downstream uses zero-extend via
            // `ensure_word_type` when needed.
            BinOp::Lt => build_cmp(context, inkwell::IntPredicate::ULT, lhs, rhs, "lt"),
            BinOp::Gt => build_cmp(context, inkwell::IntPredicate::UGT, lhs, rhs, "gt"),
            BinOp::Slt => build_cmp(context, inkwell::IntPredicate::SLT, lhs, rhs, "slt"),
            BinOp::Sgt => build_cmp(context, inkwell::IntPredicate::SGT, lhs, rhs, "sgt"),
            BinOp::Eq => build_cmp(context, inkwell::IntPredicate::EQ, lhs, rhs, "eq"),
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

    /// Gets or creates an outlined keccak256 slot wrapper function for a constant slot hash.
    ///
    /// Each wrapper is `noinline (i256 word0) -> i256` that calls
    /// `__revive_keccak256_two_words(word0, CONSTANT_SLOT)`.
    /// This avoids materializing the large i256 constant at every call site.
    fn get_or_create_keccak256_slot_wrapper(
        &mut self,
        slot_const: IntValue<'ctx>,
        context: &PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        let const_str = slot_const.print_to_string().to_string();

        if let Some(&wrapper_fn) = self.keccak256_slot_wrappers.get(&const_str) {
            return Ok(wrapper_fn);
        }

        let wrapper_name = format!("__keccak256_slot_{}", self.keccak256_slot_wrappers.len());

        // Create the wrapper function type: (i256) -> i256
        let word_type = context.word_type();
        let fn_type = word_type.fn_type(&[word_type.into()], false);

        let wrapper_fn = context.module().add_function(
            &wrapper_name,
            fn_type,
            Some(inkwell::module::Linkage::Internal),
        );

        // Set NoInline + OptimizeForSize attributes
        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let optsize_attr = context.llvm().create_enum_attribute(
            revive_llvm_context::PolkaVMAttribute::OptimizeForSize as u32,
            0,
        );
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        wrapper_fn.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        wrapper_fn.add_attribute(inkwell::attributes::AttributeLoc::Function, optsize_attr);
        wrapper_fn.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        // Save current builder position
        let saved_block = context.basic_block();

        // Build the wrapper function body
        let entry_block = context.llvm().append_basic_block(wrapper_fn, "entry");
        context.set_basic_block(entry_block);

        let word0_param = wrapper_fn.get_nth_param(0).unwrap().into_int_value();

        // Get the __revive_keccak256_two_words function
        let keccak_fn = context
            .get_function(
                revive_llvm_context::PolkaVMKeccak256TwoWordsFunction::NAME,
                false,
            )
            .expect("__revive_keccak256_two_words should be declared");

        let result = context
            .build_call(
                keccak_fn.borrow().declaration(),
                &[word0_param.into(), slot_const.into()],
                "keccak256_slot_result",
            )
            .expect("keccak256_two_words should return a value");

        context.build_return(Some(&result));

        // Restore builder position
        context.set_basic_block(saved_block);

        self.keccak256_slot_wrappers.insert(const_str, wrapper_fn);

        Ok(wrapper_fn)
    }

    /// Converts a storage key Value to a PolkaVMArgument, using global constants for
    /// constant keys to avoid materializing the same 256-bit constant at every call site.
    ///
    /// When the key is a constant i256, creates a global constant (cached by value) and
    /// returns a Pointer argument pointing to it. This eliminates the alloca+store pattern
    /// that `as_pointer()` would otherwise emit at each storage access site.
    fn value_to_storage_key_argument(
        &mut self,
        value: &Value,
        context: &PolkaVMContext<'ctx>,
    ) -> Result<PolkaVMArgument<'ctx>> {
        let llvm_val = self.translate_value(value)?;
        if !llvm_val.is_int_value() {
            return Ok(PolkaVMArgument::value(llvm_val));
        }

        let int_val = llvm_val.into_int_value();
        let word_val = self.ensure_word_type(context, int_val, "storage_key")?;

        // Only use global constants for actual constants (not runtime values)
        if !word_val.is_const() {
            return Ok(PolkaVMArgument::value(word_val.as_basic_value_enum()));
        }

        // Only use global constants for large constants (>64 bits).
        // Small constants (0, 1, etc.) are cheap to materialize inline and the
        // rodata overhead would exceed the savings on small contracts.
        if Self::try_extract_const_u64(word_val).is_some() {
            return Ok(PolkaVMArgument::value(word_val.as_basic_value_enum()));
        }

        // Use the constant's string representation as cache key
        let const_str = word_val.print_to_string().to_string();

        if let Some(&global_ptr) = self.storage_key_globals.get(&const_str) {
            // Reuse existing global constant
            let pointer = revive_llvm_context::PolkaVMPointer::new(
                context.word_type(),
                Default::default(),
                global_ptr,
            );
            Ok(PolkaVMArgument::pointer(
                pointer,
                "storage_key_global".into(),
            ))
        } else {
            // Create a new global constant
            let global_name = format!("__storage_key_{}", self.storage_key_globals.len());
            let global = context.module().add_global(
                context.word_type(),
                Some(inkwell::AddressSpace::default()),
                &global_name,
            );
            global.set_linkage(inkwell::module::Linkage::Internal);
            global.set_constant(true);
            global.set_initializer(&word_val);
            global.set_alignment(32);

            let global_ptr = global.as_pointer_value();
            self.storage_key_globals.insert(const_str, global_ptr);

            let pointer = revive_llvm_context::PolkaVMPointer::new(
                context.word_type(),
                Default::default(),
                global_ptr,
            );
            Ok(PolkaVMArgument::pointer(
                pointer,
                "storage_key_global".into(),
            ))
        }
    }
}

/// Emits an `icmp <pred> lhs, rhs` and returns the i1 result as a `BasicValueEnum`.
fn build_cmp<'ctx>(
    context: &mut PolkaVMContext<'ctx>,
    pred: inkwell::IntPredicate,
    lhs: IntValue<'ctx>,
    rhs: IntValue<'ctx>,
    name: &str,
) -> Result<BasicValueEnum<'ctx>> {
    let cmp = context
        .builder()
        .build_int_compare(pred, lhs, rhs, name)
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
    Ok(cmp.as_basic_value_enum())
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
