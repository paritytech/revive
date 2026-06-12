//! LLVM code generation for the newyork IR.
//!
//! Translates newyork IR to LLVM IR via inkwell, reusing the PolkaVM context
//! infrastructure from revive-llvm-context.

use std::collections::{BTreeMap, BTreeSet};

use inkwell::types::BasicType;
use inkwell::values::{AnyValue, BasicValue, BasicValueEnum, IntValue};
use num::{ToPrimitive, Zero};
use revive_llvm_context::{
    PolkaVMArgument, PolkaVMContext, PolkaVMFunctionDeployCode, PolkaVMFunctionRuntimeCode,
    PolkaVMMemoryEffect,
};

use crate::heap_opt::HeapOptResults;
use crate::ir::{
    BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, Type, UnaryOperation, Value, ValueId,
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

/// Attaches the standard `noinline` + `minsize` pair to an outlined helper.
fn add_noinline_minsize_attrs<'ctx>(
    context: &PolkaVMContext<'ctx>,
    function: inkwell::values::FunctionValue<'ctx>,
) {
    let noinline_attr = context
        .llvm()
        .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
    let minsize_attr = context
        .llvm()
        .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
    function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
    function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);
}

/// Attaches the named `memory(...)` effect to an outlined helper.
fn add_memory_effect_attr<'ctx>(
    context: &PolkaVMContext<'ctx>,
    function: inkwell::values::FunctionValue<'ctx>,
    effect: PolkaVMMemoryEffect,
) {
    let Some(encoding) = effect.encoding() else {
        return;
    };
    let attr = context.llvm().create_enum_attribute(
        revive_llvm_context::PolkaVMAttribute::Memory as u32,
        encoding,
    );
    function.add_attribute(inkwell::attributes::AttributeLoc::Function, attr);
}

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

/// Functions with size_estimate at or above this threshold get NoInline when our
/// IR-level pass decided not to inline them. This prevents LLVM from undoing the
/// decision by inlining a large body either at every call site (multi-call) or at
/// the single site (single-call, where LLVM's MinSize-aware inliner still pulls
/// in bodies that don't amortize the saved call-instruction overhead on PolkaVM).
/// Empirical sweep on the OZ corpus settled on 91 — vs. 50: +591 bytes, vs. 100:
/// +1,481 bytes.
const LARGE_FUNCTION_NOINLINE_THRESHOLD: usize = 91;

/// Functions with size_estimate at or below this threshold get AlwaysInline when no
/// IR-level decision was made. Very small functions benefit from inlining.
const SMALL_FUNCTION_ALWAYSINLINE_THRESHOLD: usize = 8;

/// Solidity convention: the free memory pointer is stored at this heap offset.
const FREE_MEMORY_POINTER_SLOT: u64 = 0x40;

/// Default initial value of the free memory pointer — the start of the dynamic
/// heap region above Solidity's scratch space and the FMP slot itself.
const DYNAMIC_HEAP_BASE: u64 = 0x80;

/// Length in bytes of an EVM ABI function selector.
const ABI_SELECTOR_LENGTH: u64 = 4;

/// Byte offset (inside an `Error(string)` ABI-encoded revert payload, relative to
/// the start of the payload) at which the string length word is stored.
/// Equals `selector (4) + offset word (32)`.
const ERROR_STRING_LENGTH_FIELD_OFFSET: u64 =
    ABI_SELECTOR_LENGTH + revive_common::BYTE_LENGTH_WORD as u64;

/// Byte offset (inside an `Error(string)` ABI-encoded revert payload, relative to
/// the start of the payload) at which the first string-data word begins.
/// Equals `selector (4) + offset word (32) + length word (32)`.
const ERROR_STRING_FIRST_DATA_WORD_OFFSET: u64 =
    ABI_SELECTOR_LENGTH + 2 * revive_common::BYTE_LENGTH_WORD as u64;

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
    /// Whether to use the outlined `__revive_store_low_word_checked()`
    /// helper. True only when the contract has enough constant-value
    /// MStore sites whose value fits in i64 to amortise the helper's
    /// body overhead — the per-call savings are ~3 PVM instructions and
    /// the body costs ~25 bytes, so the break-even is around 8 sites.
    use_outlined_store_low_word: bool,
    /// Whether to use the outlined `__revive_store_high_word_checked()`
    /// helper for `shl(224, sel)` ABI selector patterns. Same break-even
    /// logic as low-word.
    use_outlined_store_high_word: bool,
    /// Set of ValueIds that are bound to `Expression::CallValue`.
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
    /// Map from validator function id to the asserted-fits-in mask.
    /// Populated at the start of `generate_object`. Used to emit
    /// `llvm.assume(value u<= MASK)` after each call to a validator so
    /// downstream InstCombine can fold the redundant `and(value, MASK)`
    /// the Solidity emitter places at every caller's post-validator use.
    validator_masks: BTreeMap<u32, num::BigUint>,

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
    /// Cached outlined low-word store with bounds check:
    /// `void(i32 offset, i64 value)`. Stores zero in the high 24 bytes of
    /// the slot and `bswap(value)` in the last 8 bytes (BE encoding of a
    /// value that fits in i64). Used for ByteSwap MStore with a
    /// compile-time constant whose high 192 bits are zero — avoids passing
    /// an i256 (4 register pairs) when an i64 (2 register pairs) suffices,
    /// and shrinks the function body from 4× shift+trunc+bswap+store down
    /// to 3 zero stores + one bswap+store.
    store_low_word_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
    /// Cached outlined high-word store with bounds check:
    /// `void(i32 offset, i32 selector)`. Used for the canonical Solidity
    /// ABI mstore pattern `shl(224, sel)` — value's low 224 bits are zero,
    /// only the top 4 bytes carry the selector. Body stores 32 zero bytes
    /// then overwrites the first 4 bytes with the bswapped selector.
    store_high_word_checked_fn: Option<inkwell::values::FunctionValue<'ctx>>,
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
            use_outlined_store_low_word: false,
            use_outlined_store_high_word: false,
            callvalue_value_ids: BTreeSet::new(),
            dead_callvalue_ids: BTreeSet::new(),
            storage_key_globals: BTreeMap::new(),
            validator_masks: BTreeMap::new(),
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
            store_low_word_checked_fn: None,
            store_high_word_checked_fn: None,
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
            use_outlined_store_low_word: false,
            use_outlined_store_high_word: false,
            callvalue_value_ids: BTreeSet::new(),
            dead_callvalue_ids: BTreeSet::new(),
            storage_key_globals: BTreeMap::new(),
            validator_masks: BTreeMap::new(),
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
            store_low_word_checked_fn: None,
            store_high_word_checked_fn: None,
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

        if let Some(v) = value.get_zero_extended_constant() {
            return Some(v);
        }

        let s = value.print_to_string().to_string();
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

    /// If `value` is a compile-time constant whose representation fits in
    /// u64 (i.e., the high 192 bits of an i256 are all zero), returns the
    /// low 64 bits. Returns `None` for non-constant values, oversized
    /// constants, or zero (zero is handled by the dedicated zero-store
    /// path which avoids the i64 argument entirely).
    fn value_fits_in_i64(value: IntValue<'ctx>) -> Option<u64> {
        let low = Self::try_extract_const_u64(value)?;
        if low == 0 {
            return None;
        }
        Some(low)
    }

    /// If `value` is a compile-time i256 constant of the form
    /// `selector << 224` (low 224 bits zero, top 32 bits set), returns the
    /// selector. This is the canonical Solidity ABI selector mstore
    /// pattern (`mstore(p, shl(224, 0xf92ee8a9))`).
    fn value_is_selector_shl_224(value: IntValue<'ctx>) -> Option<u32> {
        if !value.is_const() {
            return None;
        }
        let s = value.print_to_string().to_string();
        let val_str = s.strip_prefix("i256 ")?.trim();
        let big = val_str.parse::<num::BigUint>().ok()?;
        if big.is_zero() {
            return None;
        }
        let low_mask = (num::BigUint::from(1u32) << 224) - 1u32;
        if (&big & &low_mask) != num::BigUint::ZERO {
            return None;
        }
        let selector_big: num::BigUint = big >> 224u32;
        let digits = selector_big.to_u64_digits();
        if digits.is_empty() {
            return None;
        }
        if digits.len() > 1 || digits[0] > u32::MAX as u64 {
            return None;
        }
        Some(digits[0] as u32)
    }

    /// Checks if an MLoad is loading the free memory pointer (offset 0x40).
    /// The free memory pointer is a Solidity convention where mload(64) returns
    /// the next free heap address. This value is always < 2^32 on PolkaVM.
    fn is_free_pointer_load(offset: IntValue<'ctx>) -> bool {
        Self::try_extract_const_u64(offset) == Some(FREE_MEMORY_POINTER_SLOT)
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
    ///
    /// The inline modes (`InlineNative`/`InlineByteSwap`) lower to `*_unchecked` heap GEPs with no
    /// sbrk/bounds check, so they are only sound for an offset proven to lie within the fixed heap
    /// (`offset + 32 <= heap_size`). A constant offset past the heap (e.g. `mstore(0xFFFFFFF0, x)`)
    /// must NOT take an inline path — it would write out of the heap global and corrupt adjacent
    /// globals — so it falls through to the checked `ByteSwap` path, which traps out-of-gas. The
    /// `AllNative` path is already bounds-checked via `safe_truncate_int_to_xlen`.
    fn native_memory_mode(
        &self,
        context: &PolkaVMContext<'ctx>,
        offset_llvm: IntValue<'ctx>,
    ) -> NativeMemoryMode {
        if self.heap_opt.all_native() {
            return NativeMemoryMode::AllNative;
        }
        if let Some(static_val) = Self::try_extract_const_u64(offset_llvm) {
            let heap_size = context
                .heap_size()
                .get_zero_extended_constant()
                .unwrap_or(0);
            let in_range = static_val
                .checked_add(revive_common::BYTE_LENGTH_WORD as u64)
                .is_some_and(|end| end <= heap_size);
            if in_range {
                if self.heap_opt.can_use_native(static_val) {
                    return NativeMemoryMode::InlineNative;
                }
                if static_val == FREE_MEMORY_POINTER_SLOT && self.heap_opt.fmp_native_safe() {
                    return NativeMemoryMode::InlineNative;
                }
                return NativeMemoryMode::InlineByteSwap;
            }
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
    /// Backward demand narrowing (max_width) is not used here because
    /// truncating a wide value at the definition site can break overflow
    /// detection in safe_truncate_int_to_xlen and similar safety checks.
    fn inferred_width(&self, id: ValueId) -> BitWidth {
        self.type_info.inferred_width(id)
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
            context
                .builder()
                .build_int_z_extend(value, context.word_type(), name)
                .map_err(|e| CodegenError::Llvm(e.to_string()))
        } else {
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
            let b_ext = context
                .builder()
                .build_int_z_extend(b, a.get_type(), &format!("{}_ext_b", name))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
            Ok((a, b_ext))
        } else {
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

        if a_width <= 64 && b_width <= 64 {
            return self.ensure_same_type(context, a, b, "cmp");
        }

        let a_proven = Self::provable_narrow_width(a).unwrap_or(a_width);
        let b_proven = Self::provable_narrow_width(b).unwrap_or(b_width);

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

        let a_inferred = self.inferred_width(a_id).bits();
        let b_inferred = self.inferred_width(b_id).bits();
        let a_effective = a_effective.min(a_inferred);
        let b_effective = b_effective.min(b_inferred);

        let max_needed = a_effective.max(b_effective);

        let target_bits = if max_needed <= 8 {
            8
        } else if max_needed <= 32 {
            32
        } else if max_needed <= 64 {
            64
        } else if max_needed <= 128 {
            128
        } else {
            return self.ensure_same_type(context, a, b, "cmp");
        };

        let target_type = context.integer_type(target_bits as usize);

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
        let integer_value = match value {
            BasicValueEnum::IntValue(v) => v,
            _ => return Ok(value),
        };

        let value_width = integer_value.get_type().get_bit_width();

        if value_width <= 64 {
            return Ok(value);
        }

        if let Some(proven_width) = Self::provable_narrow_width(integer_value) {
            let target_bits = if proven_width <= 8 {
                8
            } else if proven_width <= 32 {
                32
            } else if proven_width <= 64 {
                64
            } else if proven_width <= 128 {
                128
            } else {
                0
            };

            if target_bits > 0 && target_bits < value_width {
                let narrow_type = context.integer_type(target_bits as usize);
                let truncated = context
                    .builder()
                    .build_int_truncate(integer_value, narrow_type, "narrow_let")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                return Ok(truncated.as_basic_value_enum());
            }
        }

        let constraint = self.type_info.get(binding_id);
        if !constraint.is_signed {
            let demand = self.type_info.use_demand_width(binding_id);
            let target_bits = match demand {
                BitWidth::I1 | BitWidth::I8 => 8,
                BitWidth::I32 => 32,
                BitWidth::I64 => 64,
                BitWidth::I128 => 128,
                _ => return Ok(value),
            };
            if target_bits < value_width {
                let narrow_type = context.integer_type(target_bits as usize);
                let truncated = context
                    .builder()
                    .build_int_truncate(integer_value, narrow_type, "demand_narrow")
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
    /// - `and %value, constant_mask`: value fits in mask's bit width
    /// - `trunc to narrow_type`: value fits in narrow_type's width
    /// - `lshr %value, constant_amount`: result fits in (input_width - amount) bits
    fn provable_narrow_width(value: IntValue<'ctx>) -> Option<u32> {
        use inkwell::values::InstructionOpcode;

        let instruction = value.as_instruction_value()?;
        match instruction.get_opcode() {
            InstructionOpcode::ZExt => {
                let operand = instruction.get_operand(0)?.value()?.into_int_value();
                let type_width = operand.get_type().get_bit_width();
                let proven = Self::provable_narrow_width(operand).unwrap_or(type_width);
                Some(proven.min(type_width))
            }
            InstructionOpcode::And => {
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
                let operand = instruction.get_operand(0)?.value()?.into_int_value();
                let src_narrow = Self::provable_narrow_width(operand)
                    .unwrap_or(operand.get_type().get_bit_width());
                Some(src_narrow.min(target_width))
            }
            InstructionOpcode::LShr => {
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
                        Some(1)
                    }
                } else {
                    None
                }
            }
            InstructionOpcode::Or => {
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
    fn try_get_small_constant(integer_value: IntValue<'ctx>) -> Option<u64> {
        if let Some(value) = integer_value.get_zero_extended_constant() {
            return Some(value);
        }
        let wide_type = integer_value.get_type();
        if wide_type.get_bit_width() > 64 && integer_value.is_const() {
            let i64_type = wide_type.get_context().i64_type();
            let truncated = integer_value.const_truncate(i64_type);
            if let Some(value) = truncated.get_zero_extended_constant() {
                let reconstructed = wide_type.const_int(value, false);
                if reconstructed == integer_value {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Returns the effective bit width needed to represent a constant integer value.
    /// For wide types (> 64 bits), checks progressively wider truncation targets.
    fn constant_effective_width(integer_value: IntValue<'ctx>) -> Option<u32> {
        if let Some(value) = integer_value.get_zero_extended_constant() {
            return Some(if value == 0 {
                1
            } else {
                64 - value.leading_zeros()
            });
        }

        let wide_type = integer_value.get_type();
        if wide_type.get_bit_width() > 64 && integer_value.is_const() {
            let i64_type = wide_type.get_context().i64_type();
            let truncated_64 = integer_value.const_truncate(i64_type);
            if let Some(value) = truncated_64.get_zero_extended_constant() {
                let reconstructed = wide_type.const_int(value, false);
                if reconstructed == integer_value {
                    return Some(if value == 0 {
                        1
                    } else {
                        64 - value.leading_zeros()
                    });
                }
            }
        }

        None
    }

    /// Returns true when forward type inference or LLVM-IR-level structural
    /// analysis proves the argument fits in `target_bits`. Used by the Call
    /// codegen to decide between a bare truncate (when provably narrow) and a
    /// checked truncate (when the high bits might be non-zero).
    fn argument_provably_fits(
        &self,
        integer_value: IntValue<'ctx>,
        argument: Value,
        target_bits: u32,
    ) -> bool {
        let inferred = self.inferred_width(argument.id);
        if inferred.bits() <= target_bits {
            return true;
        }
        if let Some(width) = Self::provable_narrow_width(integer_value) {
            if width <= target_bits {
                return true;
            }
        }
        false
    }

    /// Emits a `trunc value, target_type` guarded by a `value != zext(trunc)`
    /// overflow check. If overflow is detected, traps via `consume_all_gas`
    /// (the same trap path `safe_truncate_int_to_xlen` uses for the i256→i32
    /// case). LLVM's instcombine folds the check away when the value is
    /// already provably narrow, so emitting it unconditionally on the
    /// not-provably-fits path costs nothing on those paths.
    fn checked_truncate_to(
        &self,
        context: &mut PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        target_type: inkwell::types::IntType<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let value_type = value.get_type();
        let truncated = context
            .builder()
            .build_int_truncate(value, target_type, name)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let extended = context
            .builder()
            .build_int_z_extend(truncated, value_type, &format!("{name}_extended"))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let is_overflow = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::NE,
                value,
                extended,
                &format!("{name}_overflow"),
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let block_continue = context.append_basic_block(&format!("{name}_ok"));
        let block_invalid = context.append_basic_block(&format!("{name}_overflow_trap"));
        context.build_conditional_branch(is_overflow, block_invalid, block_continue)?;

        context.set_basic_block(block_invalid);
        context.build_runtime_call("consume_all_gas", &[]);
        context.build_unreachable();

        context.set_basic_block(block_continue);
        Ok(truncated)
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

        if value_width <= 32 {
            return Ok(value);
        }

        let inferred = self.inferred_width(source_id);

        if matches!(inferred, BitWidth::I1 | BitWidth::I8 | BitWidth::I32) {
            let i32_type = context.llvm().i32_type();
            return context
                .builder()
                .build_int_truncate(value, i32_type, name)
                .map_err(|e| CodegenError::Llvm(e.to_string()));
        }

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
        if block.get_terminator().is_some() {
            return true;
        }
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

        let current_block = context.basic_block();

        let block_name = format!("revert_shared_{const_length}");
        let revert_block = context.append_basic_block(&block_name);
        context.set_basic_block(revert_block);

        if const_length == 0 {
            revive_llvm_context::polkavm_evm_return::revert_empty_outlined(context)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        } else if self.const_exit_range_within_heap(context, 0, const_length) {
            let length_xlen = context.xlen_type().const_int(const_length, false);
            revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        } else {
            let offset_val = context.word_const(0);
            let length_val = context.word_const(const_length);
            revive_llvm_context::polkavm_evm_return::revert(context, offset_val, length_val)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        context.build_unreachable();

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

        let current_block = context.basic_block();

        let block_name = format!("panic_0x{error_code:02x}");
        let panic_block = context.append_basic_block(&block_name);
        context.set_basic_block(panic_block);

        let panic_fn = context
            .get_function("__revive_panic", false)
            .expect("ICE: __revive_panic should be declared");
        let code_val = context.word_const(error_code as u64);
        context.build_call(
            panic_fn.borrow().declaration(),
            &[code_val.into()],
            "panic_outlined",
        );
        context.build_unreachable();

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
        if let Some(&function) = self.error_string_revert_fns.get(&num_words) {
            return Ok(function);
        }

        let word_type = context.word_type();
        let function_name = format!("__revive_error_string_revert_{num_words}");

        let mut parameter_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
            vec![word_type.into()];
        for _ in 0..num_words {
            parameter_types.push(word_type.into());
        }
        let function_type = context.llvm().void_type().fn_type(&parameter_types, false);

        let function = context.module().add_function(
            &function_name,
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let length_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let word_parameters: Vec<_> = (0..num_words)
            .map(|i| {
                function
                    .get_nth_param((i + 1) as u32)
                    .unwrap()
                    .into_int_value()
            })
            .collect();

        let fmp_offset = context.word_const(FREE_MEMORY_POINTER_SLOT);
        let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .into_int_value();

        let error_selector =
            context.word_const_str_hex(revive_common::ERROR_STRING_SELECTOR_WORD_HEX);
        revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_selector)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let fmp_plus_offset_field = context
            .builder()
            .build_int_add(
                fmp,
                context.word_const(ABI_SELECTOR_LENGTH),
                "fmp_offset_field",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let string_data_offset = context.word_const(revive_common::BYTE_LENGTH_WORD as u64);
        revive_llvm_context::polkavm_evm_memory::store(
            context,
            fmp_plus_offset_field,
            string_data_offset,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let fmp_plus_length_field = context
            .builder()
            .build_int_add(
                fmp,
                context.word_const(ERROR_STRING_LENGTH_FIELD_OFFSET),
                "fmp_length_field",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        revive_llvm_context::polkavm_evm_memory::store(
            context,
            fmp_plus_length_field,
            length_parameter,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        for (i, word_parameter) in word_parameters.iter().enumerate() {
            let offset = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                + (i as u64) * revive_common::BYTE_LENGTH_WORD as u64;
            let fmp_plus_offset = context
                .builder()
                .build_int_add(fmp, context.word_const(offset), &format!("fmp_{offset:x}"))
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
            revive_llvm_context::polkavm_evm_memory::store(
                context,
                fmp_plus_offset,
                *word_parameter,
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        let total_length = ERROR_STRING_FIRST_DATA_WORD_OFFSET
            + (num_words as u64) * revive_common::BYTE_LENGTH_WORD as u64;
        let total_length_val = context.word_const(total_length);
        revive_llvm_context::polkavm_evm_return::revert(context, fmp, total_length_val)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        context.set_basic_block(saved_block);

        self.error_string_revert_fns.insert(num_words, function);
        Ok(function)
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
        if let Some(&function) = self.custom_error_revert_fns.get(&num_args) {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_name = format!("__revive_custom_error_{num_args}");

        let mut parameter_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
            Vec::with_capacity(num_args + 1);
        parameter_types.push(xlen_type.into());
        for _ in 0..num_args {
            parameter_types.push(word_type.into());
        }
        let function_type = context.llvm().void_type().fn_type(&parameter_types, false);

        let function = context.module().add_function(
            &function_name,
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let selector_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let argument_parameters: Vec<_> = (1..=num_args)
            .map(|i| function.get_nth_param(i as u32).unwrap().into_int_value())
            .collect();

        let offset_0 = context.xlen_type().const_int(0, false);
        let selector_heap_pointer = context
            .build_heap_gep_unchecked(offset_0)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_store(selector_heap_pointer.value, selector_parameter)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");

        for (i, argument_parameter) in argument_parameters.iter().enumerate() {
            let byte_offset =
                ABI_SELECTOR_LENGTH + (i as u64) * revive_common::BYTE_LENGTH_WORD as u64;
            let offset_val = context.xlen_type().const_int(byte_offset, false);
            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                context,
                offset_val,
                *argument_parameter,
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        let const_len =
            ABI_SELECTOR_LENGTH + (num_args as u64) * revive_common::BYTE_LENGTH_WORD as u64;
        let length_xlen = context.xlen_type().const_int(const_len, false);
        revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        context.set_basic_block(saved_block);

        self.custom_error_revert_fns.insert(num_args, function);
        Ok(function)
    }

    /// Gets or creates an outlined store_bswap function: void(i32 offset, i256 value).
    /// Uses unchecked heap GEP + 4× bswap.i64 + store. This avoids duplicating
    /// the bswap sequence at every variable-value InlineByteSwap store site.
    fn get_or_create_store_bswap_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.store_bswap_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let function = context.module().add_function(
            "__revive_store_bswap",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
        let value_param = function.get_nth_param(1).unwrap().into_int_value();

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
        self.store_bswap_fn = Some(function);
        Ok(function)
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
        if let Some(function) = self.store_bswap_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let function = context.module().add_function(
            "__revive_store_bswap_checked",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
        let value_param = function.get_nth_param(1).unwrap().into_int_value();
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

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

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
        self.store_bswap_checked_fn = Some(function);
        Ok(function)
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
        if let Some(function) = self.store_zero_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into()], false);
        let function = context.module().add_function(
            "__revive_store_zero_checked",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
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

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

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
        self.store_zero_checked_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates an outlined low-word store with bounds check:
    /// `void(i32 offset, i64 value)`.
    /// Stores 24 zero bytes at heap+offset and `bswap(value)` at the last
    /// 8 bytes of the 32-byte slot. Used by `MStore` in ByteSwap mode when
    /// the value is a compile-time constant whose high 192 bits are zero.
    /// vs `__revive_store_bswap_checked(i32, i256)`:
    ///   * call site passes 1 i64 (2 register pairs) instead of 1 i256
    ///     (4 register pairs) — saves ~3 PVM instructions per call site
    ///   * function body collapses 4× shift+trunc+bswap+store down to
    ///     3 zero stores + 1 bswap+store — saves ~10 PVM instructions in
    ///     the shared body.
    fn get_or_create_store_low_word_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.store_low_word_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let i64_type = context.llvm().i64_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), i64_type.into()], false);
        let function = context.module().add_function(
            "__revive_store_low_word_checked",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
        let value_param = function.get_nth_param(1).unwrap().into_int_value();
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

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

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
        let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
            .expect("llvm.bswap.i64 intrinsic exists");
        let bswap64_decl = bswap64
            .get_declaration(context.module(), &[i64_type.into()])
            .expect("bswap.i64 declaration");
        let swapped_param = context
            .builder()
            .build_call(bswap64_decl, &[value_param.into()], "swapped_low")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        let last_word_offset = xlen_type.const_int(
            (revive_common::BYTE_LENGTH_WORD - revive_common::BYTE_LENGTH_X64) as u64,
            false,
        );
        let last_word_ptr = unsafe {
            context
                .builder()
                .build_gep(
                    context.llvm().i8_type(),
                    pointer.value,
                    &[last_word_offset],
                    "last_word_ptr",
                )
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
        };
        context
            .builder()
            .build_store(last_word_ptr, swapped_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_low_word_checked_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates an outlined high-word store with bounds check:
    /// `void(i32 offset, i32 selector)`. Stores 32 zero bytes at
    /// heap+offset and `bswap(selector)` in the FIRST 4 bytes of the
    /// slot (BE encoding of `selector << 224`). This is the canonical
    /// `mstore(p, shl(224, sel))` pattern Solidity uses for ABI selectors.
    fn get_or_create_store_high_word_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.store_high_word_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let i32_type = context.llvm().i32_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), i32_type.into()], false);
        let function = context.module().add_function(
            "__revive_store_high_word_checked",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
        let selector_parameter = function.get_nth_param(1).unwrap().into_int_value();
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

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

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

        let bswap32 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i32")
            .expect("llvm.bswap.i32 intrinsic exists");
        let bswap32_decl = bswap32
            .get_declaration(context.module(), &[i32_type.into()])
            .expect("bswap.i32 declaration");
        let swapped_selector = context
            .builder()
            .build_call(bswap32_decl, &[selector_parameter.into()], "swapped_sel")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        context
            .builder()
            .build_store(pointer.value, swapped_selector)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_high_word_checked_fn = Some(function);
        Ok(function)
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
        if let Some(function) = self.return_word_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[xlen_type.into(), word_type.into()], false);
        let function = context.module().add_function(
            "__revive_return_word",
            function_type,
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
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
        let value_param = function.get_nth_param(1).unwrap().into_int_value();
        // store_bswap_checked already performs the offset bounds check; no need
        // to duplicate it in this wrapper. After the call returns we know the
        // offset is in range and can use the unchecked GEP.
        let store_fn = self.get_or_create_store_bswap_checked_fn(context)?;
        context
            .builder()
            .build_call(store_fn, &[offset_param.into(), value_param.into()], "")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
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
                xlen_type.const_zero().into(),
                offset_pointer.into(),
                length.into(),
            ],
        );
        context.build_unreachable();

        context.set_basic_block(saved_block);
        self.return_word_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates an outlined load_bswap with bounds checking:
    /// `i256(i32 offset)`. Checks `offset > (heap_size - 32)` and traps if
    /// out of bounds, then uses unchecked GEP + 4× bswap.i64 + load. Replaces
    /// sbrk-based `__revive_load_heap_word` for non-msize contracts.
    ///
    /// The body reads heap memory exclusively (no syscalls, no globals
    /// beyond `@__heap_memory`), so we attach
    /// [`PolkaVMMemoryEffect::ReadOther`] — letting GVN fold repeated
    /// `load_bswap_checked(off)` calls of the same offset across arithmetic
    /// gaps while still being invalidated by intervening heap mstores.
    fn get_or_create_load_bswap_checked_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.load_bswap_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let word_type = context.word_type();
        let function_type = word_type.fn_type(&[xlen_type.into()], false);
        let function = context.module().add_function(
            "__revive_load_bswap_checked",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        add_noinline_minsize_attrs(context, function);
        add_memory_effect_attr(context, function, PolkaVMMemoryEffect::ReadOther);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let load_block = context.llvm().append_basic_block(function, "load");

        context.set_basic_block(entry_block);
        let offset_param = function.get_nth_param(0).unwrap().into_int_value();
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

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(load_block);
        let result =
            revive_llvm_context::polkavm_evm_memory::load_bswap_unchecked(context, offset_param)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_return(Some(&result))
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.load_bswap_checked_fn = Some(function);
        Ok(function)
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
        if let Some(function) = self.exit_checked_fn {
            return Ok(function);
        }

        let xlen_type = context.xlen_type();
        let function_type = context.llvm().void_type().fn_type(
            &[xlen_type.into(), xlen_type.into(), xlen_type.into()],
            false,
        );
        let function = context.module().add_function(
            "__revive_exit_checked",
            function_type,
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
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noreturn_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let exit_block = context.llvm().append_basic_block(function, "exit");

        context.set_basic_block(entry_block);
        let flags_param = function.get_nth_param(0).unwrap().into_int_value();
        let offset_param = function.get_nth_param(1).unwrap().into_int_value();
        let length_parameter = function.get_nth_param(2).unwrap().into_int_value();

        let heap_size = context.heap_size();
        let offset_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                offset_param,
                heap_size,
                "offset_oob",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset_ok_block = context.llvm().append_basic_block(function, "offset_ok");
        context
            .builder()
            .build_conditional_branch(offset_oob, trap_block, offset_ok_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(offset_ok_block);
        let remaining = context
            .builder()
            .build_int_sub(heap_size, offset_param, "remaining")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                length_parameter,
                remaining,
                "exit_oob",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_conditional_branch(length_oob, trap_block, exit_block)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "exit_trap");
        context.build_unreachable();

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
                length_parameter.into(),
            ],
        );
        context.build_unreachable();

        context.set_basic_block(saved_block);
        self.exit_checked_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates the outlined `__revive_sload_word(i256 key) -> i256`
    /// function. Takes the storage key as an i256 value (not pointer),
    /// internally handles bswap, alloca and the GET_STORAGE syscall.
    /// Eliminates alloca+store at each call site for runtime-computed keys
    /// (e.g. keccak256 mapping results).
    ///
    /// Effect: [`PolkaVMMemoryEffect::ReadInaccessible`]. From the caller's
    /// perspective the helper only reads pallet-revive storage; the alloca
    /// writes inside the body are not externally observable and it touches no
    /// heap or argmem. LLVM GVN can therefore fold repeated
    /// `__revive_sload_word(key)` calls across heap mstores while still being
    /// invalidated by sstore wrappers that mark `inaccessiblemem: write`.
    fn get_or_create_sload_word_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.sload_word_fn {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_type = word_type.fn_type(&[word_type.into()], false);
        let function = context.module().add_function(
            "__revive_sload_word",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        add_noinline_minsize_attrs(context, function);
        add_memory_effect_attr(context, function, PolkaVMMemoryEffect::ReadInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_param = function.get_nth_param(0).unwrap().into_int_value();

        let key_bswap = context
            .build_byte_swap(key_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let key_pointer = context.build_alloca_at_entry(word_type, "sload_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sload_value");

        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let is_transient = xlen_type.const_int(0, false);
        let arguments = [
            is_transient.into(),
            key_pointer.to_int(context).into(),
            value_pointer.to_int(context).into(),
        ];
        context.build_runtime_call("get_storage_or_zero", &arguments);

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
        self.sload_word_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates the outlined `__revive_sstore_word(i256 key, i256 value)`
    /// function. Takes key and value as i256 values (not pointers), internally
    /// handles bswap, alloca and the SET_STORAGE syscall. Eliminates
    /// alloca+store at each call site for runtime-computed keys and values.
    ///
    /// Effect: [`PolkaVMMemoryEffect::WriteInaccessible`]. The only
    /// caller-visible effect is the storage write; the key/value byte-swap
    /// allocas are local, heap state is untouched, so heap loads and calldata
    /// loads survive across an sstore.
    fn get_or_create_sstore_word_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.sstore_word_fn {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_type = context
            .llvm()
            .void_type()
            .fn_type(&[word_type.into(), word_type.into()], false);
        let function = context.module().add_function(
            "__revive_sstore_word",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        add_noinline_minsize_attrs(context, function);
        add_memory_effect_attr(context, function, PolkaVMMemoryEffect::WriteInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_param = function.get_nth_param(0).unwrap().into_int_value();
        let value_param = function.get_nth_param(1).unwrap().into_int_value();

        let key_bswap = context
            .build_byte_swap(key_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let key_pointer = context.build_alloca_at_entry(word_type, "sstore_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sstore_value");

        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

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
        self.sstore_word_fn = Some(function);
        Ok(function)
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
        if let Some(function) = self.mapping_sload_fn {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_type = word_type.fn_type(&[word_type.into(), word_type.into()], false);
        let function = context.module().add_function(
            "__revive_mapping_sload",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attr = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, noinline_attr);
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, minsize_attr);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_param = function.get_nth_param(0).unwrap().into_int_value();
        let slot_param = function.get_nth_param(1).unwrap().into_int_value();

        let offset0 = xlen_type.const_int(0, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(context, offset0, key_param)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let offset32 = xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context, offset32, slot_param,
        )
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let input_pointer = context
            .build_heap_gep_unchecked(offset0)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let length = xlen_type.const_int(2 * revive_common::BYTE_LENGTH_WORD as u64, false);

        let hash_output = context.build_alloca_at_entry(word_type, "map_sload_hash");
        let value_pointer = context.build_alloca_at_entry(word_type, "map_sload_value");

        context.build_runtime_call(
            "hash_keccak_256",
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                hash_output.to_int(context).into(),
            ],
        );

        let is_transient = xlen_type.const_int(0, false);
        context.build_runtime_call(
            "get_storage_or_zero",
            &[
                is_transient.into(),
                hash_output.to_int(context).into(),
                value_pointer.to_int(context).into(),
            ],
        );

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
        self.mapping_sload_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates `__revive_mapping_sstore(i256 key, i256 slot, i256 value)`.
    /// Combines keccak256_pair + sstore in a single function, eliminating the
    /// redundant bswap pair between keccak output and sstore key input. Uses
    /// a local alloca for the keccak input — heap writes would block the
    /// effect attribute below and prevent heap-load CSE across mapping
    /// sstores.
    ///
    /// Effect: [`PolkaVMMemoryEffect::WriteInaccessible`]. The only
    /// caller-visible effect is the storage write; heap state is untouched
    /// so heap and calldata loads survive across this call.
    fn get_or_create_mapping_sstore_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.mapping_sstore_fn {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_type = context.llvm().void_type().fn_type(
            &[word_type.into(), word_type.into(), word_type.into()],
            false,
        );
        let function = context.module().add_function(
            "__revive_mapping_sstore",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        add_noinline_minsize_attrs(context, function);
        add_memory_effect_attr(context, function, PolkaVMMemoryEffect::WriteInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_param = function.get_nth_param(0).unwrap().into_int_value();
        let slot_param = function.get_nth_param(1).unwrap().into_int_value();
        let value_param = function.get_nth_param(2).unwrap().into_int_value();

        let input_buffer_type = context
            .byte_type()
            .array_type(2 * revive_common::BYTE_LENGTH_WORD as u32);
        let input_pointer = context.build_alloca_at_entry(input_buffer_type, "map_sstore_input");
        let key_pointer = revive_llvm_context::PolkaVMPointer::new(
            word_type,
            Default::default(),
            input_pointer.value,
        );
        let slot_pointer = context.build_gep(
            input_pointer,
            &[
                xlen_type.const_zero(),
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
            ],
            word_type,
            "slot_gep",
        );
        let key_swapped = context
            .build_byte_swap(key_param.into())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .build_store(key_pointer, key_swapped)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let slot_swapped = context
            .build_byte_swap(slot_param.into())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context
            .build_store(slot_pointer, slot_swapped)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let value_bswap = context
            .build_byte_swap(value_param.as_basic_value_enum())
            .map_err(|e| CodegenError::Llvm(e.to_string()))?
            .into_int_value();
        let value_pointer = context.build_alloca_at_entry(word_type, "map_sstore_value");
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let length = xlen_type.const_int(2 * revive_common::BYTE_LENGTH_WORD as u64, false);

        let hash_output = context.build_alloca_at_entry(word_type, "map_sstore_hash");

        context.build_runtime_call(
            "hash_keccak_256",
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                hash_output.to_int(context).into(),
            ],
        );

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
        self.mapping_sstore_fn = Some(function);
        Ok(function)
    }

    /// Checks if a region is a simple `revert(0, 0)` with no other side effects.
    /// The region may contain `Let` bindings (for intermediate zero literals)
    /// followed by a `Revert` statement.
    ///
    /// The revert's operands must be zero literals bound *within this region*. A `revert(off, len)`
    /// whose operands come from an enclosing scope cannot be proven empty here — treating it as zero
    /// would let the callvalue-check outline replace a data-carrying revert with empty `revert(0, 0)`,
    /// dropping the data.
    fn is_revert_zero_region(region: &crate::ir::Region) -> bool {
        let mut zero_literal_ids: std::collections::BTreeSet<u32> =
            std::collections::BTreeSet::new();
        let mut found_revert = false;
        for statement in &region.statements {
            match statement {
                Statement::Let {
                    bindings,
                    value: Expression::Literal { value, .. },
                } => {
                    if !value.is_zero() {
                        return false;
                    }
                    for binding in bindings {
                        zero_literal_ids.insert(binding.0);
                    }
                }
                Statement::Let { .. } => {
                    return false;
                }
                Statement::Revert { offset, length } => {
                    if !zero_literal_ids.contains(&offset.id.0)
                        || !zero_literal_ids.contains(&length.id.0)
                    {
                        return false;
                    }
                    found_revert = true;
                }
                _ => return false,
            }
        }
        found_revert
    }

    /// Gets or creates the outlined callvalue check + revert function:
    /// `void __revive_callvalue_check()` that checks if callvalue is nonzero
    /// and reverts with empty data if so, returning normally otherwise.
    ///
    /// Effect: [`PolkaVMMemoryEffect::ReadInaccessible`]. Callvalue is
    /// immutable for the duration of execution and reachable through
    /// pallet-revive runtime state; the alloca write inside the body is
    /// local, with no heap or argmem traffic. CSE is sound because the
    /// helper either always returns or always reverts for a given execution
    /// — the visible "effect" is just the read of the callvalue scalar.
    fn get_or_create_callvalue_check_fn(
        &mut self,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(function) = self.callvalue_check_fn {
            return Ok(function);
        }

        let void_type = context.llvm().void_type();
        let function_type = void_type.fn_type(&[], false);
        let function = context.module().add_function(
            "__revive_callvalue_check",
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        add_noinline_minsize_attrs(context, function);
        add_memory_effect_attr(context, function, PolkaVMMemoryEffect::ReadInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let is_nonzero =
            revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                .into_int_value();

        let revert_block = context.llvm().append_basic_block(function, "revert");
        let ok_block = context.llvm().append_basic_block(function, "ok");

        context.build_conditional_branch(is_nonzero, revert_block, ok_block)?;

        context.set_basic_block(revert_block);
        revive_llvm_context::polkavm_evm_return::revert_empty_outlined(context)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.build_unreachable();

        context.set_basic_block(ok_block);
        context
            .builder()
            .build_return(None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.set_basic_block(saved_block);
        self.callvalue_check_fn = Some(function);
        Ok(function)
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
        Ok(())
    }

    /// Whether a constant `[offset, offset + length)` exit range lies wholly within the fixed heap.
    ///
    /// The dedup exit blocks ([`Self::get_or_create_return_block`] /
    /// [`Self::get_or_create_revert_block`]) lower an in-range constant range through `seal_return`
    /// over an UNCHECKED heap GEP. That is sound only when the range is heap-covered: a constant past
    /// the heap (e.g. `return(0xFFFFFFF0, 0x20)`) would read one-past-heap into the returndata — an
    /// information leak — where EVM zero-expands / runs out of gas. Out-of-range ranges must take the
    /// bounds-checked path instead. Heap coverage also subsumes the silent-truncation guard: since
    /// `heap_size <= 2^32`, a covered range's offset and length both fit in xlen.
    fn const_exit_range_within_heap(
        &self,
        context: &PolkaVMContext<'ctx>,
        offset: u64,
        length: u64,
    ) -> bool {
        let heap_size = context
            .heap_size()
            .get_zero_extended_constant()
            .unwrap_or(0);
        offset
            .checked_add(length)
            .is_some_and(|end| end <= heap_size)
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

        let current_block = context.basic_block();

        let block_name = format!("return_shared_{const_offset:x}_{const_length:x}");
        let return_block = context.append_basic_block(&block_name);
        context.set_basic_block(return_block);

        let is_deploy = matches!(
            context.code_type(),
            Some(revive_llvm_context::PolkaVMCodeType::Deploy)
        );

        if !self.has_msize
            && !is_deploy
            && self.const_exit_range_within_heap(context, const_offset, const_length)
        {
            let offset_xlen = context.xlen_type().const_int(const_offset, false);
            let length_xlen = context.xlen_type().const_int(const_length, false);
            self.emit_exit_unchecked(context, offset_xlen, length_xlen, false)?;
        } else {
            let offset_val = context.word_const(const_offset);
            let length_val = context.word_const(const_length);
            revive_llvm_context::polkavm_evm_return::r#return(context, offset_val, length_val)
                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        }

        context.build_unreachable();

        context.set_basic_block(current_block);

        self.return_blocks.insert(key, return_block);
        Ok(return_block)
    }

    /// Detects a Solidity validator: single-param void function whose body is
    /// `if iszero(eq(v, and(v, M))) { revert/panic }` where `M = 2^N - 1`.
    /// Returns the mask if the body matches.
    fn extract_validator_mask(function: &Function) -> Option<num::BigUint> {
        use num::Zero;

        if function.parameters.len() != 1 || !function.returns.is_empty() {
            return None;
        }
        let param_id = function.parameters[0].0 .0;
        let stmts = &function.body.statements;
        if stmts.len() < 5 {
            return None;
        }

        let mut constants: BTreeMap<u32, num::BigUint> = BTreeMap::new();
        let mut and_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        let mut eq_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        let mut iszero_results: BTreeMap<u32, u32> = BTreeMap::new();

        for statement in stmts {
            match statement {
                Statement::Let { bindings, value } => {
                    if bindings.len() != 1 {
                        continue;
                    }
                    let bid = bindings[0].0;
                    match value {
                        Expression::Literal { value, .. } => {
                            constants.insert(bid, value.clone());
                        }
                        Expression::Binary {
                            operation: BinaryOperation::And,
                            lhs,
                            rhs,
                        } => {
                            and_results.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        Expression::Binary {
                            operation: BinaryOperation::Eq,
                            lhs,
                            rhs,
                        } => {
                            eq_results.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        Expression::Unary {
                            operation: UnaryOperation::IsZero,
                            operand,
                        } => {
                            iszero_results.insert(bid, operand.id.0);
                        }
                        _ => {}
                    }
                }
                Statement::If {
                    condition,
                    then_region,
                    else_region,
                    ..
                } => {
                    if else_region.is_some() {
                        return None;
                    }
                    if !Self::then_region_aborts(then_region) {
                        return None;
                    }
                    let neg_id = iszero_results.get(&condition.id.0)?;
                    let (eq_lhs, eq_rhs) = eq_results.get(neg_id)?;
                    let &(and_lhs, and_rhs) = if eq_lhs == &param_id {
                        and_results.get(eq_rhs)?
                    } else if eq_rhs == &param_id {
                        and_results.get(eq_lhs)?
                    } else {
                        return None;
                    };
                    let mask = if and_lhs == param_id {
                        constants.get(&and_rhs)?
                    } else if and_rhs == param_id {
                        constants.get(&and_lhs)?
                    } else {
                        return None;
                    };
                    if mask.is_zero() {
                        return None;
                    }
                    let next = mask + num::BigUint::from(1u32);
                    if (&next & mask) != num::BigUint::zero() {
                        return None;
                    }
                    return Some(mask.clone());
                }
                _ => return None,
            }
        }
        None
    }

    /// Emits `llvm.assume(value u<= MASK)` after a validator call. This
    /// gives LLVM the range constraint without inlining the validator body.
    /// `value` is the LLVM value passed as the validator's single argument.
    fn emit_validator_assume(
        &self,
        context: &mut PolkaVMContext<'ctx>,
        value: BasicValueEnum<'ctx>,
        mask: &num::BigUint,
    ) -> Result<()> {
        let integer_value = match value {
            BasicValueEnum::IntValue(v) => v,
            _ => return Ok(()),
        };
        let value_width = integer_value.get_type().get_bit_width();
        let mask_bits = mask.bits() as u32;
        if mask_bits >= value_width {
            return Ok(());
        }
        let mask_str = mask.to_str_radix(16);
        let mask_const = integer_value
            .get_type()
            .const_int_from_string(&mask_str, inkwell::types::StringRadix::Hexadecimal)
            .ok_or_else(|| CodegenError::Llvm(format!("invalid validator mask: {mask}")))?;
        let cmp = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::ULE,
                integer_value,
                mask_const,
                "validator_assume_cmp",
            )
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        let assume = inkwell::intrinsics::Intrinsic::find("llvm.assume")
            .expect("ICE: llvm.assume intrinsic exists");
        let assume_decl = assume
            .get_declaration(context.module(), &[])
            .expect("ICE: llvm.assume declaration");
        context
            .builder()
            .build_call(assume_decl, &[cmp.into()], "validator_assume")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        Ok(())
    }

    fn then_region_aborts(region: &crate::ir::Region) -> bool {
        for statement in &region.statements {
            match statement {
                Statement::Revert { .. } | Statement::Return { .. } => return true,
                Statement::Stop | Statement::Invalid => return true,
                Statement::Expression(Expression::Call { .. }) => continue,
                Statement::Let { .. } => continue,
                _ => return false,
            }
        }
        matches!(
            region.statements.last(),
            Some(Statement::Revert { .. })
                | Some(Statement::Return { .. })
                | Some(Statement::Stop)
                | Some(Statement::Invalid)
                | Some(Statement::Expression(Expression::Call { .. }))
        )
    }

    /// Find callvalue ValueIds that are ONLY used as conditions in
    /// `if callvalue() { revert(0,0) }` or If condition patterns.
    /// These can be skipped during codegen because __revive_callvalue_check()
    /// and __revive_callvalue_nonzero() handle reading callvalue internally.
    fn find_dead_callvalue_ids(object: &Object) -> BTreeSet<u32> {
        let mut callvalue_ids = BTreeSet::new();
        let mut used_ids = BTreeSet::new();

        Self::find_callvalue_bindings(&object.code.statements, &mut callvalue_ids);
        for function in object.functions.values() {
            Self::find_callvalue_bindings(&function.body.statements, &mut callvalue_ids);
        }

        Self::find_value_uses(&object.code.statements, &callvalue_ids, &mut used_ids);
        for function in object.functions.values() {
            Self::find_value_uses(&function.body.statements, &callvalue_ids, &mut used_ids);
        }

        callvalue_ids.difference(&used_ids).copied().collect()
    }

    fn find_callvalue_bindings(statements: &[Statement], ids: &mut BTreeSet<u32>) {
        for statement in statements {
            if let Statement::Let { bindings, value } = statement {
                if bindings.len() == 1 && matches!(value, Expression::CallValue) {
                    ids.insert(bindings[0].0);
                }
            }
            Self::for_each_nested_region(statement, |region_stmts| {
                Self::find_callvalue_bindings(region_stmts, ids);
            });
        }
    }

    /// Find uses of callvalue IDs in non-condition positions.
    /// If conditions are OK (handled by callvalue_nonzero); everything else is "used".
    fn find_value_uses(
        statements: &[Statement],
        callvalue_ids: &BTreeSet<u32>,
        used: &mut BTreeSet<u32>,
    ) {
        for statement in statements {
            match statement {
                Statement::Let { value, .. } | Statement::Expression(value) => {
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
                Statement::If { inputs, .. } => {
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
                Statement::CustomErrorRevert { arguments, .. } => {
                    for a in arguments {
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
                    initial_values,
                    condition,
                    ..
                } => {
                    for v in initial_values {
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
            Self::for_each_nested_region(statement, |region_stmts| {
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
        expression: &Expression,
        callvalue_ids: &BTreeSet<u32>,
        used: &mut BTreeSet<u32>,
    ) {
        match expression {
            Expression::Var(v) => Self::mark_if_callvalue(v.0, callvalue_ids, used),
            Expression::Binary { lhs, rhs, .. } => {
                Self::mark_if_callvalue(lhs.id.0, callvalue_ids, used);
                Self::mark_if_callvalue(rhs.id.0, callvalue_ids, used);
            }
            Expression::Unary { operand, .. }
            | Expression::Truncate { value: operand, .. }
            | Expression::ZeroExtend { value: operand, .. }
            | Expression::SignExtendTo { value: operand, .. } => {
                Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
            }
            Expression::Call { arguments, .. } => {
                for a in arguments {
                    Self::mark_if_callvalue(a.id.0, callvalue_ids, used);
                }
            }
            Expression::Keccak256 { offset, length }
            | Expression::Keccak256Pair {
                word0: offset,
                word1: length,
            }
            | Expression::MappingSLoad {
                key: offset,
                slot: length,
            } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
            }
            Expression::Keccak256Single { word0 } => {
                Self::mark_if_callvalue(word0.id.0, callvalue_ids, used);
            }
            Expression::CallDataLoad { offset } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
            }
            Expression::MLoad { offset, .. } => {
                Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
            }
            _ => {}
        }
    }

    /// Call a closure for each nested region's statements in a statement.
    fn for_each_nested_region<F: FnMut(&[Statement])>(statement: &Statement, mut f: F) {
        match statement {
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
                condition_statements,
                body,
                post,
                ..
            } => {
                f(condition_statements);
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
        fn count_in_stmts(statements: &[Statement]) -> (usize, usize) {
            let mut sloads = 0;
            let mut sstores = 0;
            for statement in statements {
                match statement {
                    Statement::Let {
                        value: Expression::MappingSLoad { .. },
                        ..
                    } => sloads += 1,
                    Statement::MappingSStore { .. } => sstores += 1,
                    _ => {}
                }
                match statement {
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
                        condition_statements,
                        body,
                        post,
                        ..
                    } => {
                        let (s, w) = count_in_stmts(condition_statements);
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
        for function in object.functions.values() {
            let (s, w) = count_in_stmts(&function.body.statements);
            total_sloads += s;
            total_sstores += w;
        }
        (total_sloads, total_sstores)
    }

    /// Counts MStore sites whose value is a compile-time constant matching
    /// the low-word or high-word patterns recognised by the specialised
    /// store helpers. Returns `(low_word_count, high_word_count)`. Used to
    /// gate the introduction of those helpers — each helper costs ~25
    /// bytes in body overhead, so we only emit it when there are enough
    /// call sites to amortise that cost (per-site savings are ~3 inst).
    fn count_constant_mstore_patterns(object: &Object) -> (usize, usize) {
        use crate::ir::for_each_statement;
        use num::Zero;

        let mut literals: BTreeMap<u32, num::BigUint> = BTreeMap::new();
        let mut record = |s: &Statement| {
            if let Statement::Let { bindings, value } = s {
                if bindings.len() == 1 {
                    if let Expression::Literal { value: lit_val, .. } = value {
                        literals.insert(bindings[0].0, lit_val.clone());
                    }
                }
            }
        };
        for_each_statement(&object.code.statements, &mut record);
        for function in object.functions.values() {
            for_each_statement(&function.body.statements, &mut record);
        }

        let classify = |value: &Value| -> Option<bool> {
            let lit_val = literals.get(&value.id.0)?;
            if lit_val.is_zero() {
                return None;
            }
            if lit_val.bits() <= 64 {
                return Some(true);
            }
            let low_mask: num::BigUint = (num::BigUint::from(1u32) << 224) - 1u32;
            if (lit_val & &low_mask).is_zero() {
                let top: num::BigUint = lit_val >> 224u32;
                let digits = top.to_u64_digits();
                if digits.len() == 1 && digits[0] <= u32::MAX as u64 {
                    return Some(false);
                }
            }
            None
        };

        let mut low_total = 0usize;
        let mut high_total = 0usize;
        let mut count = |s: &Statement| {
            if let Statement::MStore { value, .. } = s {
                match classify(value) {
                    Some(true) => low_total += 1,
                    Some(false) => high_total += 1,
                    None => {}
                }
            }
        };
        for_each_statement(&object.code.statements, &mut count);
        for function in object.functions.values() {
            for_each_statement(&function.body.statements, &mut count);
        }
        (low_total, high_total)
    }

    /// Generates LLVM IR for a complete object.
    pub fn generate_object(
        &mut self,
        object: &Object,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        self.revert_blocks.clear();
        self.return_blocks.clear();
        self.panic_blocks.clear();

        let syscall_counts = object.count_syscall_sites();
        const CALLVALUE_OUTLINE_THRESHOLD: usize = 3;
        const CALLER_OUTLINE_THRESHOLD: usize = 3;
        self.use_outlined_callvalue = syscall_counts.callvalue >= CALLVALUE_OUTLINE_THRESHOLD;
        if self.use_outlined_callvalue {
            self.dead_callvalue_ids = Self::find_dead_callvalue_ids(object);
        }
        const CALLDATALOAD_OUTLINE_THRESHOLD: usize = 20;
        self.use_outlined_calldataload =
            syscall_counts.calldataload >= CALLDATALOAD_OUTLINE_THRESHOLD;
        self.use_outlined_caller = syscall_counts.caller >= CALLER_OUTLINE_THRESHOLD;
        const CONST_STORE_PATTERN_THRESHOLD: usize = 5;
        let (low_word_sites, high_word_sites) = Self::count_constant_mstore_patterns(object);
        self.use_outlined_store_low_word = low_word_sites >= CONST_STORE_PATTERN_THRESHOLD;
        self.use_outlined_store_high_word = high_word_sites >= CONST_STORE_PATTERN_THRESHOLD;
        const MAPPING_COMBINED_THRESHOLD: usize = 9;
        let (mapping_sloads, mapping_sstores) = Self::count_mapping_ops(object);
        let combined_mapping_ops = mapping_sloads + mapping_sstores;
        self.use_outlined_mapping_sload =
            mapping_sloads > 0 && combined_mapping_ops >= MAPPING_COMBINED_THRESHOLD;
        self.use_outlined_mapping_sstore =
            mapping_sstores > 0 && combined_mapping_ops >= MAPPING_COMBINED_THRESHOLD;
        self.has_msize = object.has_msize();
        self.error_string_revert_counts = object.count_error_string_reverts();
        self.custom_error_revert_counts = object.count_custom_error_reverts();

        let is_runtime = object.name.ends_with("_deployed");
        if is_runtime {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Runtime);
        } else {
            context.set_code_type(revive_llvm_context::PolkaVMCodeType::Deploy);
        }

        context.push_function_scope();

        self.validator_masks.clear();
        for (func_id, function) in &object.functions {
            if let Some(mask) = Self::extract_validator_mask(function) {
                self.validator_masks.insert(func_id.0, mask);
            }
        }

        for (func_id, function) in &object.functions {
            self.declare_function(function, context)?;
            self.function_names.insert(func_id.0, function.name.clone());
            self.function_param_types.insert(
                func_id.0,
                function
                    .parameters
                    .iter()
                    .map(|(_, value_type)| *value_type)
                    .collect(),
            );
            self.function_return_types
                .insert(func_id.0, function.returns.clone());
        }

        for function in object.functions.values() {
            self.set_inline_attributes(function, context);
        }

        for function in object.functions.values() {
            self.generate_function(function, context)?;
        }

        let function_name = if is_runtime {
            PolkaVMFunctionRuntimeCode
        } else {
            PolkaVMFunctionDeployCode
        };

        context
            .set_current_function(function_name, None, false)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        self.generate_block(&object.code, context)?;

        context
            .set_debug_location(0, 0, None)
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

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

        context.set_basic_block(context.current_function().borrow().return_block());
        context.build_return(None);

        context.pop_debug_scope();
        context.pop_function_scope();

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
        if context.get_function(&function.name, true).is_some() {
            return Ok(());
        }

        let argument_types: Vec<_> = function
            .parameters
            .iter()
            .map(|(_, value_type)| self.ir_type_to_llvm(*value_type, context))
            .collect();

        let has_narrow_returns = function.returns.iter().any(
            |value_type| matches!(value_type, Type::Int(bit_width) if *bit_width < BitWidth::I256),
        );

        let function_type = if has_narrow_returns {
            let return_types: Vec<_> = function
                .returns
                .iter()
                .map(|value_type| match value_type {
                    Type::Int(bit_width) => context.integer_type(bit_width.bits() as usize),
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

    /// Sets LLVM inline attributes on a declared function based on our custom
    /// heuristics.
    ///
    /// This provides guidance to LLVM's inliner for functions that survived
    /// our IR-level inlining pass. We use `AlwaysInline` for functions we
    /// know should be inlined, and `NoInline` only for large functions or
    /// those called from many sites where inlining would cause significant
    /// code bloat. For the `CostBenefit` decision specifically we apply
    /// `NoInline` whenever the call count is >= 3 or the body exceeds
    /// [`LARGE_FUNCTION_NOINLINE_THRESHOLD`] — otherwise MinSize-aware LLVM
    /// inlining can undo the IR-level decision and re-inline the body at
    /// every site, costing more bytes than the saved call overhead. For
    /// other functions, we let LLVM decide using its own heuristics (it
    /// already has MinSize/OptimizeForSize from `set_default_attributes`).
    fn set_inline_attributes(&self, function: &Function, context: &PolkaVMContext<'ctx>) {
        // Inline hints are a middle-end code-size heuristic; they only apply when the
        // middle-end optimizer runs. With the middle end disabled (library/post-link and
        // `-O0` builds) the backend runs `default<O0>`, so we leave inlining to its defaults
        // rather than pinning `AlwaysInline`/`NoInline` here.
        if !context.optimizer_settings().is_middle_end_enabled() {
            return;
        }

        let declaration = match context.get_function(&function.name, true) {
            Some(func_ref) => func_ref.borrow().declaration(),
            None => return,
        };

        let ir_decision = self.inline_decisions.get(&function.id.0).copied();

        let attr = match ir_decision {
            Some(crate::InlineDecision::AlwaysInline) => {
                Some(revive_llvm_context::PolkaVMAttribute::AlwaysInline)
            }
            Some(crate::InlineDecision::NeverInline) => {
                if function.size_estimate >= LARGE_FUNCTION_NOINLINE_THRESHOLD {
                    Some(revive_llvm_context::PolkaVMAttribute::NoInline)
                } else {
                    None
                }
            }
            Some(crate::InlineDecision::CostBenefit) => {
                let force_noinline = function.call_count >= 3
                    || function.size_estimate >= LARGE_FUNCTION_NOINLINE_THRESHOLD;
                if force_noinline {
                    Some(revive_llvm_context::PolkaVMAttribute::NoInline)
                } else {
                    None
                }
            }
            None => {
                if function.size_estimate <= SMALL_FUNCTION_ALWAYSINLINE_THRESHOLD {
                    Some(revive_llvm_context::PolkaVMAttribute::AlwaysInline)
                } else {
                    None
                }
            }
        };

        if let Some(attr) = attr {
            revive_llvm_context::PolkaVMFunction::set_attributes(
                context.llvm(),
                declaration,
                &[attr],
                true,
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
        let internal_name = format!(
            "{}_{}",
            function.name,
            context
                .code_type()
                .map(|c| format!("{:?}", c))
                .unwrap_or_default()
        );

        if self.generated_functions.contains(&internal_name) {
            return Ok(());
        }
        self.generated_functions.insert(internal_name);

        let saved_revert_blocks = std::mem::take(&mut self.revert_blocks);
        let saved_return_blocks = std::mem::take(&mut self.return_blocks);
        let saved_panic_blocks = std::mem::take(&mut self.panic_blocks);

        let saved_values = std::mem::take(&mut self.values);
        let saved_callvalue_ids = std::mem::take(&mut self.callvalue_value_ids);
        let saved_return_types =
            std::mem::replace(&mut self.current_return_types, function.returns.clone());

        context.set_current_function(&function.name, None, true)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        for (index, (parameter_id, parameter_type)) in function.parameters.iter().enumerate() {
            let parameter_value = context.current_function().borrow().get_nth_param(index);
            let stored_value = match parameter_type {
                Type::Int(width) if *width < BitWidth::I256 => {
                    let narrow_value = parameter_value.into_int_value();
                    context
                        .builder()
                        .build_int_z_extend(
                            narrow_value,
                            context.word_type(),
                            &format!("parameter_{}_extend", index),
                        )
                        .map_err(|e| anyhow::anyhow!("LLVM error: {e}"))?
                        .as_basic_value_enum()
                }
                _ => parameter_value,
            };
            self.set_value(*parameter_id, stored_value);
        }

        let zero = context.word_const(0).as_basic_value_enum();
        for ret_id in &function.return_values_initial {
            self.set_value(*ret_id, zero);
        }
        for ret_id in &function.return_values {
            self.set_value(*ret_id, zero);
        }

        self.generate_block(&function.body, context)?;

        match context.current_function().borrow().r#return() {
            revive_llvm_context::PolkaVMFunctionReturn::None => {}
            revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                if !function.return_values.is_empty() {
                    if let Ok(return_value) = self.get_value(function.return_values[0]) {
                        let return_value = if return_value.is_int_value() {
                            let integer_value = return_value.into_int_value();
                            match function.returns.first() {
                                Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => {
                                    let target = context.integer_type(bit_width.bits() as usize);
                                    let val_bits = integer_value.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if val_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(integer_value, target, "ret_narrow")
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else if val_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(integer_value, target, "ret_widen")
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else {
                                        integer_value.as_basic_value_enum()
                                    }
                                }
                                _ => self
                                    .ensure_word_type(context, integer_value, "return_value")?
                                    .as_basic_value_enum(),
                            }
                        } else {
                            return_value
                        };
                        context.build_store(pointer, return_value)?;
                    }
                }
            }
            revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                let field_types: Vec<_> = (0..size)
                    .map(|i| match function.returns.get(i) {
                        Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => context
                            .integer_type(bit_width.bits() as usize)
                            .as_basic_type_enum(),
                        _ => context.word_type().as_basic_type_enum(),
                    })
                    .collect();
                let struct_type = context.structure_type(&field_types);
                let mut struct_val = struct_type.get_undef();
                for (i, ret_id) in function.return_values.iter().enumerate() {
                    if let Ok(return_value) = self.get_value(*ret_id) {
                        let return_value = if return_value.is_int_value() {
                            let integer_value = return_value.into_int_value();
                            match function.returns.get(i) {
                                Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => {
                                    let target = context.integer_type(bit_width.bits() as usize);
                                    let val_bits = integer_value.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if val_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(
                                                integer_value,
                                                target,
                                                &format!("ret_narrow_{}", i),
                                            )
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else if val_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(
                                                integer_value,
                                                target,
                                                &format!("ret_widen_{}", i),
                                            )
                                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                            .as_basic_value_enum()
                                    } else {
                                        integer_value.as_basic_value_enum()
                                    }
                                }
                                _ => self
                                    .ensure_word_type(
                                        context,
                                        integer_value,
                                        &format!("ret_val_{}", i),
                                    )?
                                    .as_basic_value_enum(),
                            }
                        } else {
                            return_value
                        };
                        struct_val = context
                            .builder()
                            .build_insert_value(struct_val, return_value, i as u32, "ret_insert")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?
                            .into_struct_value();
                    }
                }
                context.build_store(pointer, struct_val.as_basic_value_enum())?;
            }
        }

        let return_block = context.current_function().borrow().return_block();
        context.build_unconditional_branch(return_block);
        context.set_basic_block(return_block);

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
    /// Detects `MStore(off, value) [+ Let(vN, Literal(32))] + Return(off, 32)` → `return_word`:
    /// combines a bswap-store and seal_return into a single noreturn function call,
    /// eliminating one function call and one redundant bounds check per site.
    fn generate_statement_list(
        &mut self,
        statements: &[Statement],
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        let mut i = 0;
        while i < statements.len() {
            if let Some(skip) = self.try_match_return_word(statements, i, context)? {
                i += skip;
                continue;
            }
            self.generate_statement(&statements[i], context)?;
            i += 1;
        }
        Ok(())
    }

    /// Tries to match and generate a combined return_word pattern.
    /// Returns `Ok(Some(skip_count))` if matched, `Ok(None)` if no match.
    ///
    /// Patterns:
    /// - `MStore(off, value) + Return(off, 32)` (2 statements)
    /// - `MStore(off, value) + Let(vN, Literal(32)) + Return(off, vN)` (3 statements)
    ///
    /// Requirements: same offset ValueId, length = 32, ByteSwap mode, !msize, !deploy.
    fn try_match_return_word(
        &mut self,
        statements: &[Statement],
        i: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<Option<usize>> {
        if i >= statements.len() {
            return Ok(None);
        }

        let (store_offset, store_value) = match &statements[i] {
            Statement::MStore { offset, value, .. } => (offset, value),
            _ => return Ok(None),
        };

        if self.has_msize {
            return Ok(None);
        }
        if matches!(
            context.code_type(),
            Some(revive_llvm_context::PolkaVMCodeType::Deploy)
        ) {
            return Ok(None);
        }

        let (ret_offset, ret_length, skip) = if i + 1 < statements.len() {
            if let Statement::Return { offset, length } = &statements[i + 1] {
                (offset, length, 2)
            } else if i + 2 < statements.len() {
                if let Statement::Return { offset, length } = &statements[i + 2] {
                    if let Statement::Let {
                        bindings,
                        value: Expression::Literal { .. },
                    } = &statements[i + 1]
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

        if store_offset.id != ret_offset.id {
            return Ok(None);
        }

        let offset_val = self.translate_value(store_offset)?.into_int_value();
        if Self::try_extract_const_u64(offset_val).is_some() {
            return Ok(None);
        }

        if skip == 3 {
            if let Statement::Let {
                value: Expression::Literal { value, .. },
                ..
            } = &statements[i + 1]
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

        let function = self.get_or_create_return_word_fn(context)?;
        context
            .builder()
            .build_call(function, &[offset_xlen.into(), value_val.into()], "")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        context.build_unreachable();
        let dead_block = context.append_basic_block("return_word_dead");
        context.set_basic_block(dead_block);

        Ok(Some(skip))
    }

    /// Generates LLVM IR for a statement.
    fn generate_statement(
        &mut self,
        statement: &Statement,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        let statement_kind = match statement {
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
            Statement::Expression(_) => "Expression",
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

        if let Err(e) = self.generate_statement_inner(statement, context) {
            return Err(CodegenError::Llvm(format!(
                "Error in {} statement: {}",
                statement_kind, e
            )));
        }
        Ok(())
    }

    /// Inner implementation of generate_statement.
    fn generate_statement_inner(
        &mut self,
        statement: &Statement,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<()> {
        match statement {
            Statement::Let { bindings, value } => {
                if bindings.len() == 1 && matches!(value, Expression::CallValue) {
                    self.callvalue_value_ids.insert(bindings[0].0);
                    if self.dead_callvalue_ids.contains(&bindings[0].0) {
                        return Ok(());
                    }
                }

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

                let llvm_value = self.generate_expression(value, context, demand)?;
                if bindings.len() == 1 {
                    let binding_id = bindings[0];
                    let llvm_value =
                        self.try_narrow_let_binding(context, llvm_value, binding_id)?;
                    self.set_value(binding_id, llvm_value);
                } else {
                    let struct_val = llvm_value.into_struct_value();
                    for (index, binding) in bindings.iter().enumerate() {
                        let field = context
                            .builder()
                            .build_extract_value(struct_val, index as u32, &format!("{}", index))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        let field = if field.is_int_value() {
                            let integer_value = field.into_int_value();
                            if integer_value.get_type().get_bit_width() < 256 {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        integer_value,
                                        context.word_type(),
                                        &format!("{}_extend", index),
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                integer_value.as_basic_value_enum()
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
                let value_val = self.ensure_word_type(context, value_val, "mstore_val")?;

                match self.native_memory_mode(context, offset_val) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_val,
                            "mstore_offset_xlen",
                        )?;
                        context.build_store_native(offset_xlen, value_val)?;
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_val,
                            "mstore_offset_xlen",
                        )?;
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        let is_fmp_store = Self::try_extract_const_u64(offset_val)
                            == Some(FREE_MEMORY_POINTER_SLOT)
                            && matches!(region, MemoryRegion::FreePointerSlot)
                            && !self.heap_opt.fmp_could_be_unbounded();
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
                        if self.has_msize {
                            let static_off = Self::try_extract_const_u64(offset_val)
                                .unwrap_or(DYNAMIC_HEAP_BASE);
                            // EVM `msize` rounds the highest accessed byte up to a full
                            // word; a full-word store touches `[off, off+32)`, so round
                            // `off+32` up to the next word (matters for unaligned offsets).
                            let word = revive_common::BYTE_LENGTH_WORD as u64;
                            let rounded = static_off.saturating_add(word).div_ceil(word) * word;
                            let min_size = context.xlen_type().const_int(rounded, false);
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
                            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                                context,
                                offset_xlen,
                                value_val,
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        } else {
                            let function = self.get_or_create_store_bswap_fn(context)?;
                            let value_word =
                                self.ensure_word_type(context, value_val, "store_bswap_val")?;
                            context
                                .builder()
                                .build_call(
                                    function,
                                    &[offset_xlen.into(), value_word.into()],
                                    "store_bswap",
                                )
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        }
                        if self.has_msize {
                            let static_off = Self::try_extract_const_u64(offset_val)
                                .unwrap_or(DYNAMIC_HEAP_BASE);
                            // EVM `msize` rounds the highest accessed byte up to a full
                            // word; a full-word store touches `[off, off+32)`, so round
                            // `off+32` up to the next word (matters for unaligned offsets).
                            let word = revive_common::BYTE_LENGTH_WORD as u64;
                            let rounded = static_off.saturating_add(word).div_ceil(word) * word;
                            let min_size = context.xlen_type().const_int(rounded, false);
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
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_val)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            if value_val.is_null() {
                                let function = self.get_or_create_store_zero_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(function, &[offset_xlen.into()], "")
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            } else if self.use_outlined_store_low_word
                                && Self::value_fits_in_i64(value_val).is_some()
                            {
                                let low = Self::value_fits_in_i64(value_val).unwrap();
                                let function =
                                    self.get_or_create_store_low_word_checked_fn(context)?;
                                let low_const = context.llvm().i64_type().const_int(low, false);
                                context
                                    .builder()
                                    .build_call(
                                        function,
                                        &[offset_xlen.into(), low_const.into()],
                                        "",
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            } else if self.use_outlined_store_high_word
                                && Self::value_is_selector_shl_224(value_val).is_some()
                            {
                                let sel = Self::value_is_selector_shl_224(value_val).unwrap();
                                let function =
                                    self.get_or_create_store_high_word_checked_fn(context)?;
                                let sel_const =
                                    context.llvm().i32_type().const_int(sel as u64, false);
                                context
                                    .builder()
                                    .build_call(
                                        function,
                                        &[offset_xlen.into(), sel_const.into()],
                                        "",
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            } else {
                                let function =
                                    self.get_or_create_store_bswap_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(
                                        function,
                                        &[offset_xlen.into(), value_val.into()],
                                        "",
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            }
                        } else {
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
                if self.use_outlined_callvalue
                    && self.callvalue_value_ids.contains(&condition.id.0)
                    && else_region.is_none()
                    && outputs.is_empty()
                    && Self::is_revert_zero_region(then_region)
                {
                    let function = self.get_or_create_callvalue_check_fn(context)?;
                    context
                        .builder()
                        .build_call(function, &[], "callvalue_check")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    return Ok(());
                }

                let cond_bool = if self.use_outlined_callvalue
                    && self.callvalue_value_ids.contains(&condition.id.0)
                {
                    revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .into_int_value()
                } else {
                    let cond_val = self.translate_value(condition)?.into_int_value();
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

                let mut phi_incoming: Vec<(
                    Vec<BasicValueEnum<'ctx>>,
                    inkwell::basic_block::BasicBlock<'ctx>,
                )> = Vec::new();

                if let Some(else_region) = else_region {
                    let else_block = context.append_basic_block("if_else");
                    context.build_conditional_branch(cond_bool, then_block, else_block)?;

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
                    let entry_block = context.basic_block();
                    context.build_conditional_branch(cond_bool, then_block, join_block)?;

                    let mut else_yields = Vec::new();
                    for (i, input_val) in inputs.iter().enumerate() {
                        else_yields.push(self.translate_value_as_word(
                            input_val,
                            context,
                            &format!("input_yield_{}", i),
                        )?);
                    }
                    phi_incoming.push((else_yields, entry_block));

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

                if phi_incoming.len() >= 2 {
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
                    let (yields, _) = &phi_incoming[0];
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
                    for output_id in outputs.iter() {
                        self.set_value(
                            *output_id,
                            context.word_type().get_undef().as_basic_value_enum(),
                        );
                    }
                    context
                        .builder()
                        .build_unreachable()
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
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

                // Soundness: case labels are materialized at the scrutinee's type via
                // `const_int_arbitrary_precision`, which truncates a label wider than
                // that type. If type inference narrowed the scrutinee (e.g. to i64 via an
                // `and(x, 0xff..ff)` mask) below the width of some case label, two labels
                // congruent modulo 2^(scrut width) would collapse — producing a duplicate
                // LLVM switch case (verifier error) or matching a value EVM never would.
                // Zero-extend the scrutinee to hold every label before lowering. This is
                // value-preserving: the scrutinee was only narrowed because it provably
                // fits its current width, so the high bits are zero.
                let max_label_bits = cases
                    .iter()
                    .map(|case| case.value.bits() as u32)
                    .max()
                    .unwrap_or(0);
                if max_label_bits > scrut_val.get_type().get_bit_width() {
                    scrut_val = context
                        .builder()
                        .build_int_z_extend(scrut_val, context.word_type(), "switch_widen")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                }

                let scrut_type = scrut_val.get_type();
                let join_block = context.append_basic_block("switch_join");

                let mut case_blocks = Vec::new();
                for (index, case) in cases.iter().enumerate() {
                    let case_block = context.append_basic_block(&format!("switch_case_{}", index));
                    let digits = case.value.to_u64_digits();
                    let case_val = if digits.is_empty() {
                        scrut_type.const_zero()
                    } else {
                        scrut_type.const_int_arbitrary_precision(&digits)
                    };
                    case_blocks.push((case_val, case_block, &case.body));
                }

                let default_block = context.append_basic_block("switch_default");

                let switch_cases: Vec<_> = case_blocks
                    .iter()
                    .map(|(value, block, _)| (*value, *block))
                    .collect();
                context
                    .builder()
                    .build_switch(scrut_val, default_block, &switch_cases)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                let mut all_yields: Vec<(
                    Vec<BasicValueEnum<'ctx>>,
                    inkwell::basic_block::BasicBlock<'ctx>,
                )> = Vec::new();

                for (index, (_, case_block, body)) in case_blocks.into_iter().enumerate() {
                    context.set_basic_block(case_block);
                    self.generate_region(body, context)?;
                    let end_block = context.basic_block();

                    if !Self::block_is_unreachable(end_block) {
                        let mut yields = Vec::new();
                        for (yield_idx, yield_val) in body.yields.iter().enumerate() {
                            match self.translate_value_as_word(
                                yield_val,
                                context,
                                &format!("case_{}_yield_{}", index, yield_idx),
                            ) {
                                Ok(v) => yields.push(v),
                                Err(e) => {
                                    return Err(CodegenError::Llvm(format!(
                                        "Switch case {} yield {}: {:?} - {}",
                                        index, yield_idx, yield_val.id, e
                                    )));
                                }
                            }
                        }
                        context.build_unconditional_branch(join_block);
                        all_yields.push((yields, end_block));
                    } else if end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }
                }

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
                    let (yields, _) = &all_yields[0];
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < yields.len() {
                            self.set_value(*output_id, yields[i]);
                        }
                    }
                } else {
                    for output_id in outputs.iter() {
                        self.set_value(
                            *output_id,
                            context.word_type().get_undef().as_basic_value_enum(),
                        );
                    }
                    context
                        .builder()
                        .build_unreachable()
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let dead_block = context.append_basic_block("switch_dead");
                    context.set_basic_block(dead_block);
                }
            }

            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
            } => {
                let mut init_llvm_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, initial_value) in initial_values.iter().enumerate() {
                    init_llvm_values.push(self.translate_value_as_word(
                        initial_value,
                        context,
                        &format!("for_init_{}", i),
                    )?);
                }

                let entry_block = context.basic_block();
                let cond_block = context.append_basic_block("for_cond");
                let body_block = context.append_basic_block("for_body");
                let continue_landing = context.append_basic_block("for_continue_landing");
                let post_block = context.append_basic_block("for_post");
                let join_block = context.append_basic_block("for_join");

                context.build_unconditional_branch(cond_block);
                context.set_basic_block(cond_block);

                let mut loop_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let mut loop_phi_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, _loop_var) in loop_variables.iter().enumerate() {
                    let phi = context
                        .builder()
                        .build_phi(context.word_type(), &format!("loop_var_{}", i))
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    if i < init_llvm_values.len() {
                        phi.add_incoming(&[(&init_llvm_values[i], entry_block)]);
                    }

                    loop_phi_values.push(phi.as_basic_value());
                    loop_phis.push(phi);
                }

                for (i, loop_var) in loop_variables.iter().enumerate() {
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

                for statement in condition_statements {
                    self.generate_statement(statement, context)?;
                }

                let cond_val = self
                    .generate_expression(condition, context, None)?
                    .into_int_value();
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

                let cond_eval_block = context.basic_block();

                context.set_basic_block(join_block);
                let mut join_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let has_loop_vars = !loop_variables.is_empty();
                if has_loop_vars {
                    for i in 0..loop_variables.len() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("join_phi_{}", i))
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        join_phis.push(phi);
                    }
                }

                context.set_basic_block(cond_eval_block);
                context.build_conditional_branch(cond_bool, body_block, join_block)?;
                if has_loop_vars {
                    for (i, phi) in join_phis.iter().enumerate() {
                        phi.add_incoming(&[(&loop_phi_values[i], cond_eval_block)]);
                    }
                }

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

                context.push_loop(body_block, continue_landing, join_block);

                self.for_loop_post_phis.push(ForLoopPostPhis {
                    phis: landing_phis.clone(),
                    loop_var_phi_values: loop_phi_values.clone(),
                });

                self.for_loop_break_phis.push(ForLoopBreakPhis {
                    phis: join_phis.clone(),
                    loop_var_phi_values: loop_phi_values.clone(),
                });

                context.set_basic_block(body_block);
                self.generate_region(body, context)?;

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

                context.build_unconditional_branch(continue_landing);

                for (phi, yield_val) in landing_phis.iter().zip(body_yield_vals.iter()) {
                    phi.add_incoming(&[(yield_val, body_end_block)]);
                }

                self.for_loop_post_phis.pop();
                self.for_loop_break_phis.pop();

                context.set_basic_block(post_block);

                if has_body_yields {
                    for (i, phi) in landing_phis.iter().enumerate() {
                        if i < post_input_variables.len() {
                            self.set_value(post_input_variables[i], phi.as_basic_value());
                        }
                    }
                }

                self.generate_region(post, context)?;

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

                if has_loop_vars {
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < join_phis.len() {
                            self.set_value(*output_id, join_phis[i].as_basic_value());
                        }
                    }
                } else {
                    for (i, output_id) in outputs.iter().enumerate() {
                        if i < loop_phis.len() {
                            self.set_value(*output_id, loop_phis[i].as_basic_value());
                        }
                    }
                }
            }

            Statement::Break { values } => {
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
                let unreachable = context.append_basic_block("break_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Continue { values } => {
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
                match context.current_function().borrow().r#return() {
                    revive_llvm_context::PolkaVMFunctionReturn::None => {}
                    revive_llvm_context::PolkaVMFunctionReturn::Primitive { pointer } => {
                        if !return_values.is_empty() {
                            if let Ok(return_value) = self.translate_value(&return_values[0]) {
                                let return_value = if return_value.is_int_value() {
                                    let integer_value = return_value.into_int_value();
                                    match self.current_return_types.first() {
                                        Some(Type::Int(bit_width))
                                            if *bit_width < BitWidth::I256 =>
                                        {
                                            let target =
                                                context.integer_type(bit_width.bits() as usize);
                                            let val_bits = integer_value.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if val_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        integer_value,
                                                        target,
                                                        "leave_narrow",
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else if val_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        integer_value,
                                                        target,
                                                        "leave_widen",
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else {
                                                integer_value.as_basic_value_enum()
                                            }
                                        }
                                        _ => self
                                            .ensure_word_type(
                                                context,
                                                integer_value,
                                                "leave_ret_val",
                                            )?
                                            .as_basic_value_enum(),
                                    }
                                } else {
                                    return_value
                                };
                                context.build_store(pointer, return_value)?;
                            }
                        }
                    }
                    revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, size } => {
                        let field_types: Vec<_> = (0..size)
                            .map(|i| match self.current_return_types.get(i) {
                                Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => {
                                    context
                                        .integer_type(bit_width.bits() as usize)
                                        .as_basic_type_enum()
                                }
                                _ => context.word_type().as_basic_type_enum(),
                            })
                            .collect();
                        let struct_type = context.structure_type(&field_types);
                        let mut struct_val = struct_type.get_undef();
                        for (i, return_value) in return_values.iter().enumerate() {
                            if let Ok(value) = self.translate_value(return_value) {
                                let value = if value.is_int_value() {
                                    let integer_value = value.into_int_value();
                                    match self.current_return_types.get(i) {
                                        Some(Type::Int(bit_width))
                                            if *bit_width < BitWidth::I256 =>
                                        {
                                            let target =
                                                context.integer_type(bit_width.bits() as usize);
                                            let val_bits = integer_value.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if val_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        integer_value,
                                                        target,
                                                        &format!("leave_narrow_{}", i),
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else if val_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        integer_value,
                                                        target,
                                                        &format!("leave_widen_{}", i),
                                                    )
                                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                                    .as_basic_value_enum()
                                            } else {
                                                integer_value.as_basic_value_enum()
                                            }
                                        }
                                        _ => self
                                            .ensure_word_type(
                                                context,
                                                integer_value,
                                                &format!("leave_ret_val_{}", i),
                                            )?
                                            .as_basic_value_enum(),
                                    }
                                } else {
                                    value
                                };
                                struct_val = context
                                    .builder()
                                    .build_insert_value(struct_val, value, i as u32, "ret_insert")
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
                    let offset_xlen = context
                        .safe_truncate_int_to_xlen(offset_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let flags = context.xlen_type().const_int(1, false);
                    let function = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            function,
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
                    let offset_xlen = context
                        .safe_truncate_int_to_xlen(offset_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_val)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    let flags = context.xlen_type().const_int(0, false);
                    let function = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            function,
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

                if count >= 2 {
                    let function = self.get_or_create_error_string_revert_fn(num_words, context)?;

                    let length_val = context.word_const(*length as u64);
                    let mut arguments: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![length_val.into()];
                    for word in data {
                        let word_val = context.word_const_str_hex(&word.to_str_radix(16));
                        arguments.push(word_val.into());
                    }

                    context
                        .builder()
                        .build_call(function, &arguments, "error_string_revert")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                } else {
                    let fmp_offset = context.word_const(FREE_MEMORY_POINTER_SLOT);
                    let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                        .into_int_value();

                    let error_selector =
                        context.word_const_str_hex(revive_common::ERROR_STRING_SELECTOR_WORD_HEX);
                    revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_selector)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    let fmp_plus_offset_field = context
                        .builder()
                        .build_int_add(
                            fmp,
                            context.word_const(ABI_SELECTOR_LENGTH),
                            "fmp_offset_field",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_plus_offset_field,
                        context.word_const(revive_common::BYTE_LENGTH_WORD as u64),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    let fmp_plus_length_field = context
                        .builder()
                        .build_int_add(
                            fmp,
                            context.word_const(ERROR_STRING_LENGTH_FIELD_OFFSET),
                            "fmp_length_field",
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_plus_length_field,
                        context.word_const(*length as u64),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    for (i, word) in data.iter().enumerate() {
                        let offset = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                            + (i as u64) * revive_common::BYTE_LENGTH_WORD as u64;
                        let fmp_plus_offset = context
                            .builder()
                            .build_int_add(
                                fmp,
                                context.word_const(offset),
                                &format!("fmp_{offset:x}"),
                            )
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        let word_val = context.word_const_str_hex(&word.to_str_radix(16));
                        revive_llvm_context::polkavm_evm_memory::store(
                            context,
                            fmp_plus_offset,
                            word_val,
                        )
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    }

                    let total_length = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                        + (num_words as u64) * revive_common::BYTE_LENGTH_WORD as u64;
                    revive_llvm_context::polkavm_evm_return::revert(
                        context,
                        fmp,
                        context.word_const(total_length),
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                }

                let dead_block = context.append_basic_block("error_string_dead");
                context.set_basic_block(dead_block);
            }

            Statement::CustomErrorRevert {
                selector,
                arguments,
            } => {
                let num_args = arguments.len();
                let count = self
                    .custom_error_revert_counts
                    .get(&num_args)
                    .copied()
                    .unwrap_or(0);

                if count >= 3 {
                    let function = self.get_or_create_custom_error_revert_fn(num_args, context)?;

                    let selector_high32 = (selector >> 224u32)
                        .iter_u32_digits()
                        .next()
                        .unwrap_or(0)
                        .swap_bytes();
                    let selector_val = context.xlen_type().const_int(selector_high32 as u64, false);
                    let mut call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![selector_val.into()];
                    for argument in arguments {
                        let arg_val = self.translate_value(argument)?.into_int_value();
                        let arg_val =
                            self.ensure_word_type(context, arg_val, "custom_error_arg")?;
                        call_args.push(arg_val.into());
                    }

                    context
                        .builder()
                        .build_call(function, &call_args, "custom_error_revert")
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                    context.build_unreachable();
                } else {
                    let selector_val = context.word_const_str_hex(&selector.to_str_radix(16));
                    let offset_0 = context.xlen_type().const_int(0, false);
                    revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                        context,
                        offset_0,
                        selector_val,
                    )
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?;

                    for (i, argument) in arguments.iter().enumerate() {
                        let arg_val = self.translate_value(argument)?.into_int_value();
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
                                let value = self.translate_value(&v)?.into_int_value();
                                self.ensure_word_type(context, value, "call_value")
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
                            vec![],
                            false,
                        )?
                    }
                    CallKind::CallCode => {
                        unreachable!("CallCode is handled above")
                    }
                    CallKind::StaticCall => revive_llvm_context::polkavm_evm_call::call(
                        context,
                        gas_val,
                        address_val,
                        None,
                        args_offset_val,
                        args_length_val,
                        ret_offset_val,
                        ret_length_val,
                        vec![],
                        true,
                    )?,
                    CallKind::DelegateCall => revive_llvm_context::polkavm_evm_call::delegate_call(
                        context,
                        gas_val,
                        address_val,
                        args_offset_val,
                        args_length_val,
                        ret_offset_val,
                        ret_length_val,
                        vec![],
                    )?,
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
                let topic_vals: Vec<BasicValueEnum<'ctx>> = topics
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let value = self.translate_value(t)?.into_int_value();
                        let value =
                            self.ensure_word_type(context, value, &format!("log_topic_{}", i))?;
                        Ok(value.as_basic_value_enum())
                    })
                    .collect::<Result<_>>()?;

                {
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
                if matches!(
                    context.code_type(),
                    Some(revive_llvm_context::PolkaVMCodeType::Runtime)
                ) {
                    return Err(CodegenError::Unsupported(
                        "The `CODECOPY` instruction is not supported in the runtime code".into(),
                    ));
                }
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

            Statement::Expression(expression) => {
                let _ = self.generate_expression(expression, context, None)?;
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
                    let hash_val =
                        if slot_val.is_const() && Self::try_extract_const_u64(slot_val).is_none() {
                            let wrapper_fn =
                                self.get_or_create_keccak256_slot_wrapper(slot_val, context)?;
                            let function_type = context
                                .word_type()
                                .fn_type(&[context.word_type().into()], false);
                            context
                                .builder()
                                .build_indirect_call(
                                    function_type,
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
                let value = self.translate_value(value)?.into_int_value();
                let value = self.ensure_word_type(context, value, "immutable_val")?;
                revive_llvm_context::polkavm_evm_immutable::store(context, index, value)?;
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
    fn generate_expression(
        &mut self,
        expression: &Expression,
        context: &mut PolkaVMContext<'ctx>,
        demand_bits: Option<u32>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match expression {
            Expression::Literal { value, .. } => {
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

            Expression::Var(id) => self.get_value(*id),

            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let lhs_val = self.translate_value(lhs)?.into_int_value();
                let rhs_val = self.translate_value(rhs)?.into_int_value();

                match operation {
                    BinaryOperation::Lt | BinaryOperation::Gt | BinaryOperation::Eq => {
                        let (lhs_cmp, rhs_cmp) =
                            self.try_narrow_comparison(context, lhs_val, rhs_val, lhs.id, rhs.id)?;
                        self.generate_binop(*operation, lhs_cmp, rhs_cmp, context)
                    }
                    BinaryOperation::Slt | BinaryOperation::Sgt => {
                        // Signed comparisons must run at full width. A narrowed
                        // operand is provably non-negative (newyork never narrows
                        // signed values), so a set top bit at the narrow width is
                        // not a sign bit — comparing at that width misreads it as
                        // negative (e.g. 1 in i1 is -1, 0xC8 in i8 is -56), which
                        // diverges from EVM's 256-bit signed comparison. Zero-extend
                        // both operands to the full word before comparing.
                        let lhs_val = self.ensure_word_type(context, lhs_val, "scmp_lhs")?;
                        let rhs_val = self.ensure_word_type(context, rhs_val, "scmp_rhs")?;
                        self.generate_binop(*operation, lhs_val, rhs_val, context)
                    }

                    BinaryOperation::And | BinaryOperation::Or | BinaryOperation::Xor => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 64, "dnbit_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 64, "dnbit_r")?;
                                return self.generate_binop(*operation, lhs_val, rhs_val, context);
                            }
                            if db <= 128 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 128, "dnbit128_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 128, "dnbit128_r")?;
                                return self.generate_binop(*operation, lhs_val, rhs_val, context);
                            }
                        }
                        let (lhs_val, rhs_val) =
                            self.ensure_same_type(context, lhs_val, rhs_val, "bitwise")?;
                        self.generate_binop(*operation, lhs_val, rhs_val, context)
                    }

                    BinaryOperation::Add | BinaryOperation::Sub | BinaryOperation::Mul => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 64, "dnarith_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 64, "dnarith_r")?;
                                return self.generate_binop(*operation, lhs_val, rhs_val, context);
                            }
                            if db <= 128 {
                                let lhs_val =
                                    self.ensure_exact_width(context, lhs_val, 128, "dnarith128_l")?;
                                let rhs_val =
                                    self.ensure_exact_width(context, rhs_val, 128, "dnarith128_r")?;
                                return self.generate_binop(*operation, lhs_val, rhs_val, context);
                            }
                        }

                        let lhs_inferred = self.inferred_width(lhs.id);
                        let rhs_inferred = self.inferred_width(rhs.id);
                        let max_operand = lhs_inferred.max(rhs_inferred);

                        let result_fits_i64 = match operation {
                            BinaryOperation::Add => {
                                crate::type_inference::widen_by_one(max_operand).bits() <= 64
                            }
                            BinaryOperation::Sub => false,
                            BinaryOperation::Mul => {
                                crate::type_inference::double_width(max_operand).bits() <= 64
                            }
                            _ => unreachable!(),
                        };

                        let result_fits_i128 =
                            max_operand.bits() <= 64 && !matches!(operation, BinaryOperation::Sub);

                        if result_fits_i64 {
                            let (lhs_val, rhs_val) =
                                self.ensure_min_width(context, lhs_val, rhs_val, 64, "arith")?;
                            self.generate_binop(*operation, lhs_val, rhs_val, context)
                        } else if result_fits_i128 {
                            let (lhs_val, rhs_val) =
                                self.ensure_min_width(context, lhs_val, rhs_val, 128, "arith128")?;
                            self.generate_binop(*operation, lhs_val, rhs_val, context)
                        } else {
                            let lhs_val = self.ensure_word_type(context, lhs_val, "arith_lhs")?;
                            let rhs_val = self.ensure_word_type(context, rhs_val, "arith_rhs")?;
                            self.generate_binop(*operation, lhs_val, rhs_val, context)
                        }
                    }

                    BinaryOperation::Div | BinaryOperation::Mod => {
                        let lhs_width = lhs_val.get_type().get_bit_width();
                        let rhs_width = rhs_val.get_type().get_bit_width();
                        if lhs_width <= 64 && rhs_width <= 64 {
                            let (lhs_val, rhs_val) =
                                self.ensure_same_type(context, lhs_val, rhs_val, "narrow_divmod")?;
                            self.generate_narrow_divmod(*operation, lhs_val, rhs_val, context)
                        } else {
                            let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                            let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                            self.generate_binop(*operation, lhs_val, rhs_val, context)
                        }
                    }

                    BinaryOperation::Shl => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                if let Some(shift) = Self::try_get_small_constant(lhs_val) {
                                    if shift >= 64 {
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
                        self.generate_binop(*operation, lhs_val, rhs_val, context)
                    }

                    BinaryOperation::Shr => {
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
                        self.generate_binop(*operation, lhs_val, rhs_val, context)
                    }

                    _ => {
                        let lhs_val = self.ensure_word_type(context, lhs_val, "binop_lhs")?;
                        let rhs_val = self.ensure_word_type(context, rhs_val, "binop_rhs")?;
                        self.generate_binop(*operation, lhs_val, rhs_val, context)
                    }
                }
            }

            Expression::Ternary { operation, a, b, n } => {
                let a_val = self.translate_value(a)?.into_int_value();
                let b_val = self.translate_value(b)?.into_int_value();
                let n_val = self.translate_value(n)?.into_int_value();
                let a_val = self.ensure_word_type(context, a_val, "ternary_a")?;
                let b_val = self.ensure_word_type(context, b_val, "ternary_b")?;
                let n_val = self.ensure_word_type(context, n_val, "ternary_n")?;

                match operation {
                    BinaryOperation::AddMod => Ok(revive_llvm_context::polkavm_evm_math::add_mod(
                        context, a_val, b_val, n_val,
                    )?),
                    BinaryOperation::MulMod => Ok(revive_llvm_context::polkavm_evm_math::mul_mod(
                        context, a_val, b_val, n_val,
                    )?),
                    _ => Err(CodegenError::Unsupported(format!(
                        "Ternary operation {:?}",
                        operation
                    ))),
                }
            }

            Expression::Unary { operation, operand } => {
                let operand_val = self.translate_value(operand)?.into_int_value();
                match operation {
                    UnaryOperation::IsZero => {
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
                    UnaryOperation::Not => {
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
                        let operand_val = self.ensure_word_type(context, operand_val, "not_op")?;
                        let all_ones = context.word_type().const_all_ones();
                        let xor_result = context
                            .builder()
                            .build_xor(operand_val, all_ones, "not_result")
                            .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                        Ok(xor_result.as_basic_value_enum())
                    }
                    UnaryOperation::Clz => {
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

            Expression::CallDataLoad { offset } => {
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

            Expression::CallValue => {
                if self.use_outlined_callvalue {
                    Ok(revive_llvm_context::polkavm_evm_ether_gas::value_outlined(
                        context,
                    )?)
                } else {
                    Ok(revive_llvm_context::polkavm_evm_ether_gas::value(context)?)
                }
            }

            Expression::Caller => {
                let value = if self.use_outlined_caller {
                    revive_llvm_context::polkavm_evm_contract_context::caller_outlined(context)?
                } else {
                    revive_llvm_context::polkavm_evm_contract_context::caller(context)?
                };
                let address_max = (num::BigUint::from(1u32) << 160) - num::BigUint::from(1u32);
                self.emit_validator_assume(context, value, &address_max)?;
                Ok(value)
            }

            Expression::Origin => {
                let value = revive_llvm_context::polkavm_evm_contract_context::origin(context)?;
                let address_max = (num::BigUint::from(1u32) << 160) - num::BigUint::from(1u32);
                self.emit_validator_assume(context, value, &address_max)?;
                Ok(value)
            }

            Expression::CallDataSize => {
                Ok(revive_llvm_context::polkavm_evm_calldata::size(context)?)
            }

            Expression::CodeSize => match context.code_type() {
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

            Expression::GasPrice => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_price(context)?)
            }

            Expression::ExtCodeSize { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "extcodesize_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::size(
                    context,
                    Some(addr_val),
                )?)
            }

            Expression::ReturnDataSize => {
                Ok(revive_llvm_context::polkavm_evm_return_data::size(context)?)
            }

            Expression::ExtCodeHash { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "extcodehash_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::hash(
                    context, addr_val,
                )?)
            }

            Expression::BlockHash { number } => {
                let num_val = self.translate_value(number)?.into_int_value();
                let num_val = self.ensure_word_type(context, num_val, "blockhash_num")?;
                Ok(
                    revive_llvm_context::polkavm_evm_contract_context::block_hash(
                        context, num_val,
                    )?,
                )
            }

            Expression::Coinbase => {
                let value = revive_llvm_context::polkavm_evm_contract_context::coinbase(context)?;
                let address_max = (num::BigUint::from(1u32) << 160) - num::BigUint::from(1u32);
                self.emit_validator_assume(context, value, &address_max)?;
                Ok(value)
            }

            Expression::Timestamp => {
                let value =
                    revive_llvm_context::polkavm_evm_contract_context::block_timestamp(context)?;
                Self::apply_range_proof(context, value, 64, "timestamp")
            }

            Expression::Number => {
                let value =
                    revive_llvm_context::polkavm_evm_contract_context::block_number(context)?;
                Self::apply_range_proof(context, value, 64, "number")
            }

            Expression::Difficulty => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::difficulty(context)?)
            }

            Expression::GasLimit => {
                Ok(revive_llvm_context::polkavm_evm_contract_context::gas_limit(context)?)
            }

            Expression::ChainId => {
                let value = revive_llvm_context::polkavm_evm_contract_context::chain_id(context)?;
                Self::apply_range_proof(context, value, 64, "chainid")
            }

            Expression::SelfBalance => Ok(
                revive_llvm_context::polkavm_evm_ether_gas::self_balance(context)?,
            ),

            Expression::BaseFee => {
                let value = revive_llvm_context::polkavm_evm_contract_context::basefee(context)?;
                Self::apply_range_proof(context, value, 128, "basefee")
            }

            Expression::BlobHash { .. } | Expression::BlobBaseFee => {
                Ok(context.word_const(0).as_basic_value_enum())
            }

            Expression::Gas => Ok(revive_llvm_context::polkavm_evm_ether_gas::gas(context)?),

            Expression::MSize => Ok(revive_llvm_context::polkavm_evm_memory::msize(context)?),

            Expression::Address => {
                let value = revive_llvm_context::polkavm_evm_contract_context::address(context)?;
                let address_max = (num::BigUint::from(1u32) << 160) - num::BigUint::from(1u32);
                self.emit_validator_assume(context, value, &address_max)?;
                Ok(value)
            }

            Expression::Balance { address } => {
                let addr_val = self.translate_value(address)?.into_int_value();
                let addr_val = self.ensure_word_type(context, addr_val, "balance_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ether_gas::balance(
                    context, addr_val,
                )?)
            }

            Expression::MLoad { offset, region } => {
                let offset_val = self.translate_value(offset)?.into_int_value();
                // EVM grows active memory (and thus `msize`) on reads as well as
                // writes. The unchecked native/byteswap load paths bypass sbrk, so
                // advance the msize watermark explicitly to cover `[offset, offset+32)`,
                // mirroring `MStore`. Without this, `mload(p)` followed by `msize()`
                // under-reports relative to EVM.
                if self.has_msize {
                    let static_off =
                        Self::try_extract_const_u64(offset_val).unwrap_or(DYNAMIC_HEAP_BASE);
                    // EVM `msize` is the highest accessed byte rounded *up* to a full
                    // word. The read touches `[off, off + WORD)`, so round `off + WORD`
                    // up to the next word boundary (matters for unaligned offsets).
                    let word = revive_common::BYTE_LENGTH_WORD as u64;
                    let rounded = static_off.saturating_add(word).div_ceil(word) * word;
                    let min_size = context.xlen_type().const_int(rounded, false);
                    context
                        .ensure_heap_size(min_size)
                        .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                }
                let is_free_pointer = matches!(region, MemoryRegion::FreePointerSlot)
                    || Self::is_free_pointer_load(offset_val);

                let loaded = match self.native_memory_mode(context, offset_val) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen =
                            self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                        context.build_load_native(offset_xlen)?
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen =
                            self.truncate_offset_to_xlen(context, offset_val, "mload_offset_xlen")?;
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        if is_free_pointer && !self.heap_opt.fmp_could_be_unbounded() {
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
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_val)
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?;
                            let function = self.get_or_create_load_bswap_checked_fn(context)?;
                            context
                                .builder()
                                .build_call(function, &[offset_xlen.into()], "checked_load")
                                .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                .try_as_basic_value()
                                .basic()
                                .expect("load_bswap_checked should return a value")
                        } else {
                            revive_llvm_context::polkavm_evm_memory::load(context, offset_val)?
                        }
                    }
                };
                if is_free_pointer && !self.heap_opt.fmp_could_be_unbounded() {
                    // Soundness gate: the post-MLoad range proof asserts
                    // `FMP < heap_size`. Iter36's win (-31k bytes on OZ)
                    // relies on this holding when Solidity's allocator
                    // updates the FMP via sbrk-style `mstore(0x40,
                    // add(mload(0x40), bounded))` or a literal initial
                    // value. Hand-written Yul / inline asm that writes
                    // a non-bounded value to 0x40 violates the
                    // assumption. `fmp_could_be_unbounded` is set by
                    // `heap_opt::analyze_statement` when any FMP store
                    // uses a value whose source isn't a recognized
                    // allocator pattern, so the proof stays sound and
                    // OZ contracts keep their codesize wins.
                    let heap_size = context
                        .heap_size()
                        .get_zero_extended_constant()
                        .unwrap_or(131072);
                    let max_fmp = heap_size.saturating_sub(1).max(1);
                    let raw_bits = 64 - max_fmp.leading_zeros();
                    let range_bits = raw_bits.clamp(8, 31);
                    Self::apply_range_proof(context, loaded, range_bits, "fmp")
                } else {
                    Ok(loaded)
                }
            }

            Expression::SLoad {
                key,
                static_slot: _,
            } => {
                let key_arg = self.value_to_storage_key_argument(key, context)?;
                if key_arg.is_register() {
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
                    Ok(revive_llvm_context::polkavm_evm_storage::load(
                        context, &key_arg,
                    )?)
                }
            }

            Expression::TLoad { key } => {
                let key_arg = self.value_to_argument(key, context)?;
                Ok(revive_llvm_context::polkavm_evm_storage::transient_load(
                    context, &key_arg,
                )?)
            }

            Expression::Call {
                function,
                arguments,
            } => {
                let function_name = self
                    .function_names
                    .get(&function.0)
                    .ok_or(CodegenError::UndefinedFunction(*function))?
                    .clone();

                let parameter_types = self.function_param_types.get(&function.0).cloned();
                let mut argument_values = Vec::new();
                for (i, argument) in arguments.iter().enumerate() {
                    let parameter_type = parameter_types
                        .as_ref()
                        .and_then(|parameter_types| parameter_types.get(i));
                    let value = match parameter_type {
                        Some(Type::Int(width)) if *width < BitWidth::I256 => {
                            let llvm_val = self.translate_value(argument)?;
                            let integer_value = llvm_val.into_int_value();
                            let target_type = context.integer_type(width.bits() as usize);
                            let argument_bits = integer_value.get_type().get_bit_width();
                            let target_bits = target_type.get_bit_width();
                            if argument_bits > target_bits {
                                // Soundness: `narrow_function_params` may narrow a
                                // parameter to I64/I32 based purely on use-site
                                // demand (e.g. the callee uses the value only as an
                                // mload offset). A bare `trunc i256 → iN` would
                                // silently drop bits and bypass the use-site
                                // `safe_truncate_int_to_xlen`. If forward type
                                // inference can't prove the argument fits, emit a
                                // checked truncate that traps on overflow.
                                if self.argument_provably_fits(
                                    integer_value,
                                    *argument,
                                    target_bits,
                                ) {
                                    context
                                        .builder()
                                        .build_int_truncate(
                                            integer_value,
                                            target_type,
                                            &format!("call_arg_narrow_{}", i),
                                        )
                                        .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                        .as_basic_value_enum()
                                } else {
                                    self.checked_truncate_to(
                                        context,
                                        integer_value,
                                        target_type,
                                        &format!("call_arg_narrow_{}", i),
                                    )?
                                    .as_basic_value_enum()
                                }
                            } else if argument_bits < target_bits {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        integer_value,
                                        target_type,
                                        &format!("call_arg_widen_{}", i),
                                    )
                                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                integer_value.as_basic_value_enum()
                            }
                        }
                        _ => self.translate_value_as_word(
                            argument,
                            context,
                            &format!("call_arg_{}", i),
                        )?,
                    };
                    argument_values.push(value);
                }

                context.set_debug_location(1, 0, None)?;

                let llvm_function = context
                    .get_function(&function_name, true)
                    .ok_or(CodegenError::UndefinedFunction(*function))?;
                let result = context.build_call(
                    llvm_function.borrow().declaration(),
                    &argument_values,
                    &format!("{}_result", function_name),
                );

                if let Some(mask) = self.validator_masks.get(&function.0) {
                    if let Some(first_arg) = argument_values.first() {
                        self.emit_validator_assume(context, *first_arg, mask)?;
                    }
                }

                let result = result.unwrap_or_else(|| context.word_const(0).as_basic_value_enum());

                let return_types = self.function_return_types.get(&function.0);
                let result = match return_types {
                    Some(return_types)
                        if return_types.len() == 1
                            && matches!(return_types[0], Type::Int(bit_width) if bit_width < BitWidth::I256) =>
                    {
                        if result.is_int_value() {
                            let integer_value = result.into_int_value();
                            if integer_value.get_type().get_bit_width() < 256 {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        integer_value,
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

            Expression::Truncate { value, to } => {
                let value = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_truncate(value, target_type, "truncate")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::ZeroExtend { value, to } => {
                let value = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_z_extend(value, target_type, "zext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::SignExtendTo { value, to } => {
                let value = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_s_extend(value, target_type, "sext")
                    .map_err(|e| CodegenError::Llvm(e.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::Keccak256 { offset, length } => {
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

            Expression::Keccak256Pair { word0, word1 } => {
                let word0_val = self.translate_value(word0)?.into_int_value();
                let word0_val = self.ensure_word_type(context, word0_val, "keccak_word0")?;
                let word1_val = self.translate_value(word1)?.into_int_value();
                let word1_val = self.ensure_word_type(context, word1_val, "keccak_word1")?;

                if word1_val.is_const() && Self::try_extract_const_u64(word1_val).is_none() {
                    let wrapper_fn =
                        self.get_or_create_keccak256_slot_wrapper(word1_val, context)?;
                    let function_type = context
                        .word_type()
                        .fn_type(&[context.word_type().into()], false);
                    let result = context
                        .builder()
                        .build_indirect_call(
                            function_type,
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

            Expression::Keccak256Single { word0 } => {
                let word0_val = self.translate_value(word0)?.into_int_value();
                let word0_val = self.ensure_word_type(context, word0_val, "keccak_word0")?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3_one_word(
                    context, word0_val,
                )?)
            }

            Expression::DataOffset { id } => {
                let argument =
                    revive_llvm_context::polkavm_evm_create::contract_hash(context, id.clone())?;
                argument
                    .access(context)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))
            }

            Expression::DataSize { id } => {
                let argument =
                    revive_llvm_context::polkavm_evm_create::header_size(context, id.clone())?;
                argument
                    .access(context)
                    .map_err(|e| CodegenError::Llvm(e.to_string()))
            }

            Expression::LoadImmutable { key } => {
                let offset = context
                    .solidity_mut()
                    .get_or_allocate_immutable(key.as_str())
                    / revive_common::BYTE_LENGTH_WORD;
                let index = context.xlen_type().const_int(offset as u64, false);
                Ok(revive_llvm_context::polkavm_evm_immutable::load(
                    context, index,
                )?)
            }

            Expression::MappingSLoad { key, slot } => {
                let key_val = self.translate_value(key)?.into_int_value();
                let key_val = self.ensure_word_type(context, key_val, "mapping_sload_key")?;
                let slot_val = self.translate_value(slot)?.into_int_value();
                let slot_val = self.ensure_word_type(context, slot_val, "mapping_sload_slot")?;

                if self.use_outlined_mapping_sload {
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
                    let hash_val =
                        if slot_val.is_const() && Self::try_extract_const_u64(slot_val).is_none() {
                            let wrapper_fn =
                                self.get_or_create_keccak256_slot_wrapper(slot_val, context)?;
                            let function_type = context
                                .word_type()
                                .fn_type(&[context.word_type().into()], false);
                            context
                                .builder()
                                .build_indirect_call(
                                    function_type,
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

            Expression::LinkerSymbol { path } => Ok(
                revive_llvm_context::polkavm_evm_call::linker_symbol(context, path)?,
            ),
        }
    }

    /// Generates a narrow (64-bit or less) unsigned division or modulo.
    /// Uses native LLVM udiv/urem instead of expensive 256-bit runtime calls.
    /// Handles division by zero (returns 0 per EVM spec).
    fn generate_narrow_divmod(
        &mut self,
        operation: BinaryOperation,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let int_type = lhs.get_type();
        let zero = int_type.const_zero();

        let is_zero = context
            .builder()
            .build_int_compare(inkwell::IntPredicate::EQ, rhs, zero, "divmod_iszero")
            .map_err(|e| CodegenError::Llvm(e.to_string()))?;

        let non_zero_block = context.append_basic_block("divmod_nonzero");
        let join_block = context.append_basic_block("divmod_join");
        let current_block = context.basic_block();

        context.build_conditional_branch(is_zero, join_block, non_zero_block)?;

        context.set_basic_block(non_zero_block);
        let result = match operation {
            BinaryOperation::Div => context
                .builder()
                .build_int_unsigned_div(lhs, rhs, "narrow_div")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?,
            BinaryOperation::Mod => context
                .builder()
                .build_int_unsigned_rem(lhs, rhs, "narrow_mod")
                .map_err(|e| CodegenError::Llvm(e.to_string()))?,
            _ => unreachable!(),
        };
        let non_zero_exit = context.basic_block();
        context.build_unconditional_branch(join_block);

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
        operation: BinaryOperation,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match operation {
            BinaryOperation::Add => Ok(revive_llvm_context::polkavm_evm_arithmetic::addition(
                context, lhs, rhs,
            )?),
            BinaryOperation::Sub => Ok(revive_llvm_context::polkavm_evm_arithmetic::subtraction(
                context, lhs, rhs,
            )?),
            BinaryOperation::Mul => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::multiplication(context, lhs, rhs)?,
            ),
            BinaryOperation::Div => Ok(revive_llvm_context::polkavm_evm_arithmetic::division(
                context, lhs, rhs,
            )?),
            BinaryOperation::SDiv => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::division_signed(context, lhs, rhs)?,
            ),
            BinaryOperation::Mod => Ok(revive_llvm_context::polkavm_evm_arithmetic::remainder(
                context, lhs, rhs,
            )?),
            BinaryOperation::SMod => Ok(
                revive_llvm_context::polkavm_evm_arithmetic::remainder_signed(context, lhs, rhs)?,
            ),
            BinaryOperation::Exp => Ok(revive_llvm_context::polkavm_evm_math::exponent(
                context, lhs, rhs,
            )?),
            BinaryOperation::And => Ok(revive_llvm_context::polkavm_evm_bitwise::and(
                context, lhs, rhs,
            )?),
            BinaryOperation::Or => Ok(revive_llvm_context::polkavm_evm_bitwise::or(
                context, lhs, rhs,
            )?),
            BinaryOperation::Xor => Ok(revive_llvm_context::polkavm_evm_bitwise::xor(
                context, lhs, rhs,
            )?),
            BinaryOperation::Shl => {
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
            BinaryOperation::Shr => {
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
            BinaryOperation::Sar => Ok(
                revive_llvm_context::polkavm_evm_bitwise::shift_right_arithmetic(
                    context, lhs, rhs,
                )?,
            ),
            BinaryOperation::Lt => build_cmp(context, inkwell::IntPredicate::ULT, lhs, rhs, "lt"),
            BinaryOperation::Gt => build_cmp(context, inkwell::IntPredicate::UGT, lhs, rhs, "gt"),
            BinaryOperation::Slt => build_cmp(context, inkwell::IntPredicate::SLT, lhs, rhs, "slt"),
            BinaryOperation::Sgt => build_cmp(context, inkwell::IntPredicate::SGT, lhs, rhs, "sgt"),
            BinaryOperation::Eq => build_cmp(context, inkwell::IntPredicate::EQ, lhs, rhs, "eq"),
            BinaryOperation::Byte => Ok(revive_llvm_context::polkavm_evm_bitwise::byte(
                context, lhs, rhs,
            )?),
            BinaryOperation::SignExtend => Ok(revive_llvm_context::polkavm_evm_math::sign_extend(
                context, lhs, rhs,
            )?),
            BinaryOperation::AddMod | BinaryOperation::MulMod => Err(CodegenError::Unsupported(
                format!("Binary call for ternary operation {:?}", operation),
            )),
        }
    }

    /// Converts an IR type to LLVM type.
    fn ir_type_to_llvm(
        &self,
        value_type: Type,
        context: &PolkaVMContext<'ctx>,
    ) -> inkwell::types::BasicTypeEnum<'ctx> {
        match value_type {
            Type::Int(width) => context
                .integer_type(width.bits() as usize)
                .as_basic_type_enum(),
            Type::Ptr(_) => context.word_type().as_basic_type_enum(),
            Type::Void => context.word_type().as_basic_type_enum(),
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
            let integer_value = llvm_val.into_int_value();
            Ok(self
                .ensure_word_type(context, integer_value, name)?
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
            let integer_value = llvm_val.into_int_value();
            let word_val = self.ensure_word_type(context, integer_value, "storage_arg")?;
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

        let word_type = context.word_type();
        let function_type = word_type.fn_type(&[word_type.into()], false);

        let wrapper_fn = context.module().add_function(
            &wrapper_name,
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

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

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(wrapper_fn, "entry");
        context.set_basic_block(entry_block);

        let word0_param = wrapper_fn.get_nth_param(0).unwrap().into_int_value();

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

        let integer_value = llvm_val.into_int_value();
        let word_val = self.ensure_word_type(context, integer_value, "storage_key")?;

        if !word_val.is_const() {
            return Ok(PolkaVMArgument::value(word_val.as_basic_value_enum()));
        }

        if Self::try_extract_const_u64(word_val).is_some() {
            return Ok(PolkaVMArgument::value(word_val.as_basic_value_enum()));
        }

        let const_str = word_val.print_to_string().to_string();

        if let Some(&global_ptr) = self.storage_key_globals.get(&const_str) {
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
