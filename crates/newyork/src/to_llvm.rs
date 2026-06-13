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
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value, ValueId,
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
    fn from(error: anyhow::Error) -> Self {
        CodegenError::Llvm(error.to_string())
    }
}

/// Result type for codegen operations.
pub type Result<T> = std::result::Result<T, CodegenError>;

/// Attaches the standard `noinline` + `minsize` pair to an outlined helper.
fn add_noinline_minsize_attrs<'ctx>(
    context: &PolkaVMContext<'ctx>,
    function: inkwell::values::FunctionValue<'ctx>,
) {
    let noinline_attribute = context
        .llvm()
        .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
    let minsize_attribute = context
        .llvm()
        .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
    function.add_attribute(
        inkwell::attributes::AttributeLoc::Function,
        noinline_attribute,
    );
    function.add_attribute(
        inkwell::attributes::AttributeLoc::Function,
        minsize_attribute,
    );
}

/// Attaches the named `memory(...)` effect to an outlined helper.
fn add_memory_effect_attribute<'ctx>(
    context: &PolkaVMContext<'ctx>,
    function: inkwell::values::FunctionValue<'ctx>,
    effect: PolkaVMMemoryEffect,
) {
    let Some(encoding) = effect.encoding() else {
        return;
    };
    let attribute = context.llvm().create_enum_attribute(
        revive_llvm_context::PolkaVMAttribute::Memory as u32,
        encoding,
    );
    function.add_attribute(inkwell::attributes::AttributeLoc::Function, attribute);
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
    loop_variable_phi_values: Vec<BasicValueEnum<'ctx>>,
}

/// Tracks phi nodes at the join block of a for loop.
/// These merge values from the normal loop exit (condition false) and break sites.
struct ForLoopBreakPhis<'ctx> {
    /// Phi nodes at the join block, one per loop-carried variable.
    phis: Vec<inkwell::values::PhiValue<'ctx>>,
    /// The loop-carried variable phi values (from the cond phi nodes).
    /// Used as fallback values when break is taken before a body yield is defined.
    loop_variable_phi_values: Vec<BasicValueEnum<'ctx>>,
}

pub struct LlvmCodegen<'ctx> {
    /// Value table: maps IR ValueId to LLVM value.
    values: BTreeMap<u32, BasicValueEnum<'ctx>>,
    /// Function table: maps IR FunctionId to function name.
    function_names: BTreeMap<u32, String>,
    /// Function parameter types: maps IR FunctionId to parameter types.
    /// Used by call sites to match argument types to narrowed parameter types.
    function_parameter_types: BTreeMap<u32, Vec<Type>>,
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
    callstored_word_ids: BTreeSet<u32>,
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
    /// Keyed by num_arguments. The selector is passed as the first parameter.
    custom_error_revert_fns: BTreeMap<usize, inkwell::values::FunctionValue<'ctx>>,
    /// Number of CustomErrorRevert sites per num_arguments.
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

/// Whether `region` aborts on every path (control never falls off its end). Used to confirm a
/// validator's failure branch diverges, so a post-validator `llvm.assume(argument u<= MASK)` is sound.
///
/// A trailing call only aborts when its callee is in `noreturn` — a call to a function that *returns*
/// (e.g. a failure handler that logs/stores and returns) leaves the argument out-of-range, so the
/// assume would be false and LLVM would fold dependent comparisons to constants. Intermediate calls to
/// returning functions are fine: control continues to the region's terminator.
fn then_region_aborts(
    region: &crate::ir::Region,
    noreturn: &std::collections::BTreeSet<u32>,
) -> bool {
    for statement in &region.statements {
        match statement {
            Statement::Revert { .. }
            | Statement::Return { .. }
            | Statement::Stop
            | Statement::Invalid => return true,
            Statement::Expression(Expression::Call { function, .. })
                if noreturn.contains(&function.0) =>
            {
                return true
            }
            Statement::Expression(Expression::Call { .. }) | Statement::Let { .. } => continue,
            _ => return false,
        }
    }
    false
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
            function_parameter_types: BTreeMap::new(),
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
            callstored_word_ids: BTreeSet::new(),
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
            function_parameter_types: BTreeMap::new(),
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
            callstored_word_ids: BTreeSet::new(),
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

        if let Some(constant) = value.get_zero_extended_constant() {
            return Some(constant);
        }

        let printed = value.print_to_string().to_string();
        if let Some(value_str) = printed.strip_prefix("i256 ") {
            if let Ok(parsed) = value_str.trim().parse::<u64>() {
                return Some(parsed);
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
        let printed = value.print_to_string().to_string();
        let value_str = printed.strip_prefix("i256 ")?.trim();
        let big = value_str.parse::<num::BigUint>().ok()?;
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let extended = context
            .builder()
            .build_int_z_extend(truncated, context.word_type(), &format!("{name}_extend"))
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        Ok(extended.as_basic_value_enum())
    }

    /// Applies the free-memory-pointer range proof to a loaded value.
    ///
    /// The post-MLoad range proof asserts `FMP < heap_size`. The codesize win
    /// from dropping the byte-order gate on this proof relies on the assertion
    /// holding when Solidity's allocator updates the FMP via sbrk-style
    /// `mstore(0x40, add(mload(0x40), bounded))` or a literal initial value.
    /// Hand-written Yul / inline asm that writes a non-bounded value to 0x40
    /// violates the assumption; `fmp_could_be_unbounded` is set by the heap
    /// analysis when any FMP store uses a value whose source isn't a
    /// recognized allocator pattern, so the proof stays sound and OZ contracts
    /// keep their codesize wins.
    fn apply_free_pointer_range_proof(
        context: &PolkaVMContext<'ctx>,
        loaded: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let heap_size = context
            .heap_size()
            .get_zero_extended_constant()
            .unwrap_or(131072);
        let max_fmp = heap_size.saturating_sub(1).max(1);
        let raw_bits = 64 - max_fmp.leading_zeros();
        let range_bits = raw_bits.clamp(8, 31);
        Self::apply_range_proof(context, loaded, range_bits, "fmp")
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
        if let Some(static_value) = Self::try_extract_const_u64(offset_llvm) {
            let heap_size = context
                .heap_size()
                .get_zero_extended_constant()
                .unwrap_or(0);
            let in_range = static_value
                .checked_add(revive_common::BYTE_LENGTH_WORD as u64)
                .is_some_and(|end| end <= heap_size);
            if in_range {
                if self.heap_opt.can_use_native(static_value) {
                    return NativeMemoryMode::InlineNative;
                }
                if static_value == FREE_MEMORY_POINTER_SLOT && self.heap_opt.fmp_native_safe() {
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
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        }
    }

    /// Advances the `msize` watermark to cover a word-sized access at `offset`.
    ///
    /// EVM grows active memory (and thus `msize`) on reads as well as writes.
    /// The unchecked native/byteswap paths bypass sbrk, so the watermark is
    /// advanced explicitly to cover `[offset, offset + WORD)`. Without this,
    /// `mload(p)`/`mstore(p, _)` followed by `msize()` under-reports relative
    /// to EVM.
    ///
    /// EVM `msize` is the highest accessed byte rounded *up* to a full word; a
    /// word-sized access touches `[off, off + WORD)`, so `off + WORD` is
    /// rounded up to the next word boundary (this matters for unaligned
    /// offsets). A dynamic (non-constant) offset is treated as the heap base.
    fn advance_msize_watermark(
        &self,
        context: &PolkaVMContext<'ctx>,
        offset_value: IntValue<'ctx>,
    ) -> Result<()> {
        if self.has_msize {
            let static_offset =
                Self::try_extract_const_u64(offset_value).unwrap_or(DYNAMIC_HEAP_BASE);
            let word = revive_common::BYTE_LENGTH_WORD as u64;
            let rounded = static_offset.saturating_add(word).div_ceil(word) * word;
            let min_size = context.xlen_type().const_int(rounded, false);
            context
                .ensure_heap_size(min_size)
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        }
        Ok(())
    }

    /// Zero-extends a switch scrutinee so its type can hold every case label.
    ///
    /// Case labels are materialized at the scrutinee's type via
    /// `const_int_arbitrary_precision`, which truncates a label wider than that
    /// type. If type inference narrowed the scrutinee (e.g. to i64 via an
    /// `and(x, 0xff..ff)` mask) below the width of some case label, two labels
    /// congruent modulo `2^(scrut width)` would collapse — producing a
    /// duplicate LLVM switch case (verifier error) or matching a value EVM
    /// never would. Widening before lowering is value-preserving: the
    /// scrutinee was only narrowed because it provably fits its current width,
    /// so the high bits are zero.
    fn widen_scrutinee_for_case_labels(
        &self,
        context: &PolkaVMContext<'ctx>,
        scrutinee_value: IntValue<'ctx>,
        cases: &[SwitchCase],
    ) -> Result<IntValue<'ctx>> {
        let max_label_bits = cases
            .iter()
            .map(|case| case.value.bits() as u32)
            .max()
            .unwrap_or(0);
        if max_label_bits > scrutinee_value.get_type().get_bit_width() {
            context
                .builder()
                .build_int_z_extend(scrutinee_value, context.word_type(), "switch_widen")
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        } else {
            Ok(scrutinee_value)
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
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        } else {
            context
                .builder()
                .build_int_truncate(value, context.word_type(), name)
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        }
    }

    /// Ensures two values have the same type by extending the narrower one.
    /// Returns both values at the wider type.
    fn ensure_same_type(
        &self,
        context: &PolkaVMContext<'ctx>,
        first: IntValue<'ctx>,
        second: IntValue<'ctx>,
        name: &str,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let first_width = first.get_type().get_bit_width();
        let second_width = second.get_type().get_bit_width();

        if first_width == second_width {
            Ok((first, second))
        } else if first_width > second_width {
            let second_extended = context
                .builder()
                .build_int_z_extend(second, first.get_type(), &format!("{}_ext_b", name))
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
            Ok((first, second_extended))
        } else {
            let first_extended = context
                .builder()
                .build_int_z_extend(first, second.get_type(), &format!("{}_ext_a", name))
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
            Ok((first_extended, second))
        }
    }

    /// Ensures both operands are extended to at least the given minimum width.
    /// This is used for arithmetic operations where the result needs first certain
    /// width to avoid modular arithmetic wrapping at the wrong boundary.
    fn ensure_min_width(
        &self,
        context: &PolkaVMContext<'ctx>,
        first: IntValue<'ctx>,
        second: IntValue<'ctx>,
        min_width: u32,
        name: &str,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let target_width = min_width
            .max(first.get_type().get_bit_width())
            .max(second.get_type().get_bit_width());
        let target_type = context.integer_type(target_width as usize);

        let first_extended = if first.get_type().get_bit_width() < target_width {
            context
                .builder()
                .build_int_z_extend(first, target_type, &format!("{}_ext_a", name))
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else {
            first
        };
        let second_extended = if second.get_type().get_bit_width() < target_width {
            context
                .builder()
                .build_int_z_extend(second, target_type, &format!("{}_ext_b", name))
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else {
            second
        };
        Ok((first_extended, second_extended))
    }

    /// Adjusts first single value to an exact target width: truncates if wider,
    /// zero-extends if narrower, returns unchanged if already the target width.
    fn ensure_exact_width(
        &self,
        context: &PolkaVMContext<'ctx>,
        value: IntValue<'ctx>,
        target_bits: u32,
        name: &str,
    ) -> Result<IntValue<'ctx>> {
        let current_bits = value.get_type().get_bit_width();
        if current_bits == target_bits {
            Ok(value)
        } else if current_bits < target_bits {
            let target_type = context.integer_type(target_bits as usize);
            context
                .builder()
                .build_int_z_extend(value, target_type, name)
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        } else {
            let target_type = context.integer_type(target_bits as usize);
            context
                .builder()
                .build_int_truncate(value, target_type, name)
                .map_err(|error| CodegenError::Llvm(error.to_string()))
        }
    }

    /// Tries to narrow comparison operands to first smaller type when both are
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
        first: IntValue<'ctx>,
        second: IntValue<'ctx>,
        first_id: ValueId,
        second_id: ValueId,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let first_width = first.get_type().get_bit_width();
        let second_width = second.get_type().get_bit_width();

        if first_width <= 64 && second_width <= 64 {
            return self.ensure_same_type(context, first, second, "cmp");
        }

        let first_proven = Self::provable_narrow_width(first).unwrap_or(first_width);
        let second_proven = Self::provable_narrow_width(second).unwrap_or(second_width);

        let first_effective = if first.is_const() {
            Self::constant_effective_width(first)
                .unwrap_or(first_proven)
                .min(first_proven)
        } else {
            first_proven
        };
        let second_effective = if second.is_const() {
            Self::constant_effective_width(second)
                .unwrap_or(second_proven)
                .min(second_proven)
        } else {
            second_proven
        };

        let first_inferred = self.inferred_width(first_id).bits();
        let second_inferred = self.inferred_width(second_id).bits();
        let first_effective = first_effective.min(first_inferred);
        let second_effective = second_effective.min(second_inferred);

        let max_needed = first_effective.max(second_effective);
        let target_bits = if max_needed <= 8 {
            8
        } else if max_needed <= 32 {
            32
        } else if max_needed <= 64 {
            64
        } else if max_needed <= 128 {
            128
        } else {
            return self.ensure_same_type(context, first, second, "cmp");
        };

        let target_type = context.integer_type(target_bits as usize);

        let first_narrow = if first_width > target_bits {
            context
                .builder()
                .build_int_truncate(first, target_type, "cmp_narrow_a")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else if first_width < target_bits {
            context
                .builder()
                .build_int_z_extend(first, target_type, "cmp_ext_a")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else {
            first
        };

        let second_narrow = if second_width > target_bits {
            context
                .builder()
                .build_int_truncate(second, target_type, "cmp_narrow_b")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else if second_width < target_bits {
            context
                .builder()
                .build_int_z_extend(second, target_type, "cmp_ext_b")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        } else {
            second
        };

        Ok((first_narrow, second_narrow))
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
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                    (Some(width0), Some(width1)) => Some(width0.min(width1)),
                    (Some(width0), None) => Some(width0),
                    (None, Some(width1)) => Some(width1),
                    (None, None) => None,
                }
            }
            InstructionOpcode::Trunc => {
                let target_width = instruction.get_type().into_int_type().get_bit_width();
                let operand = instruction.get_operand(0)?.value()?.into_int_value();
                let source_narrow = Self::provable_narrow_width(operand)
                    .unwrap_or(operand.get_type().get_bit_width());
                Some(source_narrow.min(target_width))
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
                    (Some(width0), Some(width1)) => Some(width0.max(width1)),
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
                    (Some(width0), Some(width1)) => {
                        let result_width = width0.max(width1) + 1;
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
                    (Some(width0), Some(width1)) => {
                        let result_width = width0 + width1;
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
    ///
    /// `narrow_function_parameters` may narrow a parameter to I64/I32 based purely
    /// on use-site demand (e.g. the callee uses the value only as an mload
    /// offset). A bare `trunc i256 → iN` would silently drop bits and bypass
    /// the use-site `safe_truncate_int_to_xlen`, so when this check cannot
    /// prove the argument fits the caller emits a checked truncate that traps
    /// on overflow.
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let extended = context
            .builder()
            .build_int_z_extend(truncated, value_type, &format!("{name}_extended"))
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let is_overflow = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::NE,
                value,
                extended,
                &format!("{name}_overflow"),
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                .map_err(|error| CodegenError::Llvm(error.to_string()));
        }

        if matches!(inferred, BitWidth::I64) && value_width > 64 {
            let i64_type = context.llvm().i64_type();
            return context
                .builder()
                .build_int_truncate(value, i64_type, name)
                .map_err(|error| CodegenError::Llvm(error.to_string()));
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
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        } else if self.const_exit_range_within_heap(context, 0, const_length) {
            let length_xlen = context.xlen_type().const_int(const_length, false);
            revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        } else {
            let offset_value = context.word_const(0);
            let length_value = context.word_const(const_length);
            revive_llvm_context::polkavm_evm_return::revert(context, offset_value, length_value)
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
        let code_value = context.word_const(error_code as u64);
        context.build_call(
            panic_fn.borrow().declaration(),
            &[code_value.into()],
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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noreturn_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let length_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let word_parameters: Vec<_> = (0..num_words)
            .map(|index| {
                function
                    .get_nth_param((index + 1) as u32)
                    .unwrap()
                    .into_int_value()
            })
            .collect();

        let fmp_offset = context.word_const(FREE_MEMORY_POINTER_SLOT);
        let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .into_int_value();

        let error_selector =
            context.word_const_str_hex(revive_common::ERROR_STRING_SELECTOR_WORD_HEX);
        revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_selector)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let fmp_plus_offset_field = context
            .builder()
            .build_int_add(
                fmp,
                context.word_const(ABI_SELECTOR_LENGTH),
                "fmp_offset_field",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let string_data_offset = context.word_const(revive_common::BYTE_LENGTH_WORD as u64);
        revive_llvm_context::polkavm_evm_memory::store(
            context,
            fmp_plus_offset_field,
            string_data_offset,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let fmp_plus_length_field = context
            .builder()
            .build_int_add(
                fmp,
                context.word_const(ERROR_STRING_LENGTH_FIELD_OFFSET),
                "fmp_length_field",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        revive_llvm_context::polkavm_evm_memory::store(
            context,
            fmp_plus_length_field,
            length_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        for (index, word_parameter) in word_parameters.iter().enumerate() {
            let offset = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                + (index as u64) * revive_common::BYTE_LENGTH_WORD as u64;
            let fmp_plus_offset = context
                .builder()
                .build_int_add(fmp, context.word_const(offset), &format!("fmp_{offset:x}"))
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
            revive_llvm_context::polkavm_evm_memory::store(
                context,
                fmp_plus_offset,
                *word_parameter,
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        }

        let total_length = ERROR_STRING_FIRST_DATA_WORD_OFFSET
            + (num_words as u64) * revive_common::BYTE_LENGTH_WORD as u64;
        let total_length_value = context.word_const(total_length);
        revive_llvm_context::polkavm_evm_return::revert(context, fmp, total_length_value)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
        num_arguments: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<inkwell::values::FunctionValue<'ctx>> {
        if let Some(&function) = self.custom_error_revert_fns.get(&num_arguments) {
            return Ok(function);
        }

        let word_type = context.word_type();
        let xlen_type = context.xlen_type();
        let function_name = format!("__revive_custom_error_{num_arguments}");

        let mut parameter_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
            Vec::with_capacity(num_arguments + 1);
        parameter_types.push(xlen_type.into());
        for _ in 0..num_arguments {
            parameter_types.push(word_type.into());
        }
        let function_type = context.llvm().void_type().fn_type(&parameter_types, false);

        let function = context.module().add_function(
            &function_name,
            function_type,
            Some(inkwell::module::Linkage::Internal),
        );

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let noreturn_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noreturn_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let selector_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let argument_parameters: Vec<_> = (1..=num_arguments)
            .map(|index| {
                function
                    .get_nth_param(index as u32)
                    .unwrap()
                    .into_int_value()
            })
            .collect();

        let offset_0 = context.xlen_type().const_int(0, false);
        let selector_heap_pointer = context
            .build_heap_gep_unchecked(offset_0)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_store(selector_heap_pointer.value, selector_parameter)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");

        for (index, argument_parameter) in argument_parameters.iter().enumerate() {
            let byte_offset =
                ABI_SELECTOR_LENGTH + (index as u64) * revive_common::BYTE_LENGTH_WORD as u64;
            let offset_value = context.xlen_type().const_int(byte_offset, false);
            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                context,
                offset_value,
                *argument_parameter,
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        }

        let const_len =
            ABI_SELECTOR_LENGTH + (num_arguments as u64) * revive_common::BYTE_LENGTH_WORD as u64;
        let length_xlen = context.xlen_type().const_int(const_len, false);
        revive_llvm_context::polkavm_evm_return::revert_outlined(context, length_xlen)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context.build_unreachable();

        context.set_basic_block(saved_block);

        self.custom_error_revert_fns.insert(num_arguments, function);
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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(1).unwrap().into_int_value();

        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset_parameter,
            value_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_parameter,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(store_block);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset_parameter,
            value_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_parameter,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(store_block);
        let pointer = context.build_heap_gep_unchecked(offset_parameter)?;
        let word_type = context.word_type();
        let zero = word_type.const_zero();
        context
            .builder()
            .build_store(pointer.value, zero)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_parameter,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(store_block);
        let pointer = context.build_heap_gep_unchecked(offset_parameter)?;
        let word_type = context.word_type();
        let zero = word_type.const_zero();
        context
            .builder()
            .build_store(pointer.value, zero)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
            .expect("llvm.bswap.i64 intrinsic exists");
        let bswap64_decl = bswap64
            .get_declaration(context.module(), &[i64_type.into()])
            .expect("bswap.i64 declaration");
        let swapped_parameter = context
            .builder()
            .build_call(bswap64_decl, &[value_parameter.into()], "swapped_low")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        let last_word_offset = xlen_type.const_int(
            (revive_common::BYTE_LENGTH_WORD - revive_common::BYTE_LENGTH_X64) as u64,
            false,
        );
        let last_word_pointer = unsafe {
            context
                .builder()
                .build_gep(
                    context.llvm().i8_type(),
                    pointer.value,
                    &[last_word_offset],
                    "last_word_ptr",
                )
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
        };
        context
            .builder()
            .build_store(last_word_pointer, swapped_parameter)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let store_block = context.llvm().append_basic_block(function, "store");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let selector_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_parameter,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, store_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(store_block);
        let pointer = context.build_heap_gep_unchecked(offset_parameter)?;
        let word_type = context.word_type();
        let zero = word_type.const_zero();
        context
            .builder()
            .build_store(pointer.value, zero)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        context
            .builder()
            .build_store(pointer.value, swapped_selector)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(saved_block);
        self.store_high_word_checked_fn = Some(function);
        Ok(function)
    }

    /// Gets or creates an outlined return_word function:
    /// noreturn void(i32 offset, i256 value).
    /// Combines store_bswap_checked + exit_checked for single-word returns:
    /// bounds-checks offset, bswap-stores value at heap+offset, then seal_returns 32 bytes.
    /// Eliminates one function call per site and one redundant bounds check.
    ///
    /// `store_bswap_checked` already performs the offset bounds check, so the
    /// wrapper does not duplicate it. After that call returns the offset is
    /// known to be in range, so the subsequent GEP can be unchecked.
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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        let noreturn_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noreturn_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let store_fn = self.get_or_create_store_bswap_checked_fn(context)?;
        context
            .builder()
            .build_call(
                store_fn,
                &[offset_parameter.into(), value_parameter.into()],
                "",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let heap_pointer = context
            .build_heap_gep_unchecked(offset_parameter)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, xlen_type, "return_word_ptr")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
        add_memory_effect_attribute(context, function, PolkaVMMemoryEffect::ReadOther);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let load_block = context.llvm().append_basic_block(function, "load");

        context.set_basic_block(entry_block);
        let offset_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let heap_size = context.heap_size();
        let max_offset = context
            .builder()
            .build_int_sub(
                heap_size,
                xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false),
                "max_offset",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let out_of_bounds = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                offset_parameter,
                max_offset,
                "out_of_bounds",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(out_of_bounds, trap_block, load_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "heap_trap");
        context.build_unreachable();

        context.set_basic_block(load_block);
        let result = revive_llvm_context::polkavm_evm_memory::load_bswap_unchecked(
            context,
            offset_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_return(Some(&result))
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        let noreturn_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoReturn as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noreturn_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        let trap_block = context.llvm().append_basic_block(function, "trap");
        let exit_block = context.llvm().append_basic_block(function, "exit");

        context.set_basic_block(entry_block);
        let flags_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let offset_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let length_parameter = function.get_nth_param(2).unwrap().into_int_value();

        let heap_size = context.heap_size();
        let offset_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                offset_parameter,
                heap_size,
                "offset_oob",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let offset_ok_block = context.llvm().append_basic_block(function, "offset_ok");
        context
            .builder()
            .build_conditional_branch(offset_oob, trap_block, offset_ok_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(offset_ok_block);
        let remaining = context
            .builder()
            .build_int_sub(heap_size, offset_parameter, "remaining")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let length_oob = context
            .builder()
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                length_parameter,
                remaining,
                "exit_oob",
            )
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_conditional_branch(length_oob, trap_block, exit_block)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "exit_trap");
        context.build_unreachable();

        context.set_basic_block(exit_block);
        let heap_pointer = context
            .build_heap_gep_unchecked(offset_parameter)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, xlen_type, "exit_ptr")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context.build_runtime_call(
            "seal_return",
            &[
                flags_parameter.into(),
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
        add_memory_effect_attribute(context, function, PolkaVMMemoryEffect::ReadInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_parameter = function.get_nth_param(0).unwrap().into_int_value();

        let key_bswap = context
            .build_byte_swap(key_parameter.as_basic_value_enum())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let key_pointer = context.build_alloca_at_entry(word_type, "sload_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sload_value");

        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let is_transient = xlen_type.const_int(0, false);
        let arguments = [
            is_transient.into(),
            key_pointer.to_int(context).into(),
            value_pointer.to_int(context).into(),
        ];
        context.build_runtime_call("get_storage_or_zero", &arguments);

        let value = context
            .build_load(value_pointer, "sload_result")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context
            .builder()
            .build_return(Some(&value_bswap))
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
        add_memory_effect_attribute(context, function, PolkaVMMemoryEffect::WriteInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(1).unwrap().into_int_value();

        let key_bswap = context
            .build_byte_swap(key_parameter.as_basic_value_enum())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value_parameter.as_basic_value_enum())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let key_pointer = context.build_alloca_at_entry(word_type, "sstore_key");
        let value_pointer = context.build_alloca_at_entry(word_type, "sstore_value");

        context
            .builder()
            .build_store(key_pointer.value, key_bswap)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let slot_parameter = function.get_nth_param(1).unwrap().into_int_value();

        let offset0 = xlen_type.const_int(0, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset0,
            key_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let offset32 = xlen_type.const_int(revive_common::BYTE_LENGTH_WORD as u64, false);
        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
            context,
            offset32,
            slot_parameter,
        )
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let input_pointer = context
            .build_heap_gep_unchecked(offset0)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let value_bswap = context
            .build_byte_swap(value)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        context
            .builder()
            .build_return(Some(&value_bswap))
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
        add_memory_effect_attribute(context, function, PolkaVMMemoryEffect::WriteInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let key_parameter = function.get_nth_param(0).unwrap().into_int_value();
        let slot_parameter = function.get_nth_param(1).unwrap().into_int_value();
        let value_parameter = function.get_nth_param(2).unwrap().into_int_value();

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
            .build_byte_swap(key_parameter.into())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .build_store(key_pointer, key_swapped)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let slot_swapped = context
            .build_byte_swap(slot_parameter.into())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context
            .build_store(slot_pointer, slot_swapped)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let value_bswap = context
            .build_byte_swap(value_parameter.as_basic_value_enum())
            .map_err(|error| CodegenError::Llvm(error.to_string()))?
            .into_int_value();
        let value_pointer = context.build_alloca_at_entry(word_type, "map_sstore_value");
        context
            .builder()
            .build_store(value_pointer.value, value_bswap)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
        add_memory_effect_attribute(context, function, PolkaVMMemoryEffect::ReadInaccessible);

        let saved_block = context.basic_block();
        let entry_block = context.llvm().append_basic_block(function, "entry");
        context.set_basic_block(entry_block);

        let is_nonzero =
            revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                .into_int_value();

        let revert_block = context.llvm().append_basic_block(function, "revert");
        let ok_block = context.llvm().append_basic_block(function, "ok");

        context.build_conditional_branch(is_nonzero, revert_block, ok_block)?;

        context.set_basic_block(revert_block);
        revive_llvm_context::polkavm_evm_return::revert_empty_outlined(context)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context.build_unreachable();

        context.set_basic_block(ok_block);
        context
            .builder()
            .build_return(None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let offset_pointer = context
            .builder()
            .build_ptr_to_int(heap_pointer.value, context.xlen_type(), "exit_data_ptr")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
            let offset_value = context.word_const(const_offset);
            let length_value = context.word_const(const_length);
            revive_llvm_context::polkavm_evm_return::r#return(context, offset_value, length_value)
                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        }

        context.build_unreachable();

        context.set_basic_block(current_block);

        self.return_blocks.insert(key, return_block);
        Ok(return_block)
    }

    /// Detects a Solidity validator: single-param void function whose body is
    /// `if iszero(eq(v, and(v, M))) { revert/panic }` where `M = 2^N - 1`.
    /// Returns the mask if the body matches.
    fn extract_validator_mask(
        function: &Function,
        noreturn: &std::collections::BTreeSet<u32>,
    ) -> Option<num::BigUint> {
        use num::Zero;

        if function.parameters.len() != 1 || !function.returns.is_empty() {
            return None;
        }
        let parameter_id = function.parameters[0].0 .0;
        let statements = &function.body.statements;
        if statements.len() < 5 {
            return None;
        }

        let mut constants: BTreeMap<u32, num::BigUint> = BTreeMap::new();
        let mut and_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        let mut eq_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        let mut iszero_results: BTreeMap<u32, u32> = BTreeMap::new();

        for statement in statements {
            match statement {
                Statement::Let { bindings, value } => {
                    if bindings.len() != 1 {
                        continue;
                    }
                    let binding_id = bindings[0].0;
                    match value {
                        Expression::Literal { value, .. } => {
                            constants.insert(binding_id, value.clone());
                        }
                        Expression::Binary {
                            operation: BinaryOperation::And,
                            lhs,
                            rhs,
                        } => {
                            and_results.insert(binding_id, (lhs.id.0, rhs.id.0));
                        }
                        Expression::Binary {
                            operation: BinaryOperation::Eq,
                            lhs,
                            rhs,
                        } => {
                            eq_results.insert(binding_id, (lhs.id.0, rhs.id.0));
                        }
                        Expression::Unary {
                            operation: UnaryOperation::IsZero,
                            operand,
                        } => {
                            iszero_results.insert(binding_id, operand.id.0);
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
                    if !then_region_aborts(then_region, noreturn) {
                        return None;
                    }
                    let neg_id = iszero_results.get(&condition.id.0)?;
                    let (eq_lhs, eq_rhs) = eq_results.get(neg_id)?;
                    let &(and_lhs, and_rhs) = if eq_lhs == &parameter_id {
                        and_results.get(eq_rhs)?
                    } else if eq_rhs == &parameter_id {
                        and_results.get(eq_lhs)?
                    } else {
                        return None;
                    };
                    let mask = if and_lhs == parameter_id {
                        constants.get(&and_rhs)?
                    } else if and_rhs == parameter_id {
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        let assume = inkwell::intrinsics::Intrinsic::find("llvm.assume")
            .expect("ICE: llvm.assume intrinsic exists");
        let assume_decl = assume
            .get_declaration(context.module(), &[])
            .expect("ICE: llvm.assume declaration");
        context
            .builder()
            .build_call(assume_decl, &[cmp.into()], "validator_assume")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        Ok(())
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
            Self::for_each_nested_region(statement, |region_statements| {
                Self::find_callvalue_bindings(region_statements, ids);
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
                    for operand in inputs {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
                    }
                }
                Statement::Switch {
                    scrutinee, inputs, ..
                } => {
                    Self::mark_if_callvalue(scrutinee.id.0, callvalue_ids, used);
                    for operand in inputs {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
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
                    for topic in topics {
                        Self::mark_if_callvalue(topic.id.0, callvalue_ids, used);
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
                    if let Some(operand) = value {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
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
                    if let Some(salt_value) = salt {
                        Self::mark_if_callvalue(salt_value.id.0, callvalue_ids, used);
                    }
                }
                Statement::CustomErrorRevert { arguments, .. } => {
                    for argument in arguments {
                        Self::mark_if_callvalue(argument.id.0, callvalue_ids, used);
                    }
                }
                Statement::Leave { return_values } => {
                    for operand in return_values {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
                    }
                }
                Statement::Break { values } | Statement::Continue { values } => {
                    for operand in values {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
                    }
                }
                Statement::For {
                    initial_values,
                    condition,
                    ..
                } => {
                    for operand in initial_values {
                        Self::mark_if_callvalue(operand.id.0, callvalue_ids, used);
                    }
                    Self::collect_expr_value_refs(condition, callvalue_ids, used);
                }
                Statement::MCopy {
                    destination,
                    source,
                    length,
                } => {
                    Self::mark_if_callvalue(destination.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(source.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::SelfDestruct { address } => {
                    Self::mark_if_callvalue(address.id.0, callvalue_ids, used);
                }
                Statement::CodeCopy {
                    destination,
                    offset,
                    length,
                } => {
                    Self::mark_if_callvalue(destination.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(offset.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(length.id.0, callvalue_ids, used);
                }
                Statement::ExtCodeCopy {
                    address,
                    destination,
                    offset,
                    length,
                } => {
                    Self::mark_if_callvalue(address.id.0, callvalue_ids, used);
                    Self::mark_if_callvalue(destination.id.0, callvalue_ids, used);
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
            Self::for_each_nested_region(statement, |region_statements| {
                Self::find_value_uses(region_statements, callvalue_ids, used);
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
            Expression::Var(variable_id) => {
                Self::mark_if_callvalue(variable_id.0, callvalue_ids, used)
            }
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
                for argument in arguments {
                    Self::mark_if_callvalue(argument.id.0, callvalue_ids, used);
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
    fn for_each_nested_region<F: FnMut(&[Statement])>(statement: &Statement, mut callback: F) {
        match statement {
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                callback(&then_region.statements);
                if let Some(region) = else_region {
                    callback(&region.statements);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    callback(&case.body.statements);
                }
                if let Some(default_region) = default {
                    callback(&default_region.statements);
                }
            }
            Statement::For {
                condition_statements,
                body,
                post,
                ..
            } => {
                callback(condition_statements);
                callback(&body.statements);
                callback(&post.statements);
            }
            Statement::Block(region) => {
                callback(&region.statements);
            }
            _ => {}
        }
    }

    /// Counts MappingSLoad and MappingSStore operations separately in an object.
    fn count_mapping_operations(object: &Object) -> (usize, usize) {
        fn count_in_statements(statements: &[Statement]) -> (usize, usize) {
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
                        let (nested_sloads, nested_sstores) =
                            count_in_statements(&then_region.statements);
                        sloads += nested_sloads;
                        sstores += nested_sstores;
                        if let Some(else_region_inner) = else_region {
                            let (nested_sloads, nested_sstores) =
                                count_in_statements(&else_region_inner.statements);
                            sloads += nested_sloads;
                            sstores += nested_sstores;
                        }
                    }
                    Statement::Switch { cases, default, .. } => {
                        for case in cases {
                            let (nested_sloads, nested_sstores) =
                                count_in_statements(&case.body.statements);
                            sloads += nested_sloads;
                            sstores += nested_sstores;
                        }
                        if let Some(default_region) = default {
                            let (nested_sloads, nested_sstores) =
                                count_in_statements(&default_region.statements);
                            sloads += nested_sloads;
                            sstores += nested_sstores;
                        }
                    }
                    Statement::For {
                        condition_statements,
                        body,
                        post,
                        ..
                    } => {
                        let (nested_sloads, nested_sstores) =
                            count_in_statements(condition_statements);
                        sloads += nested_sloads;
                        sstores += nested_sstores;
                        let (nested_sloads, nested_sstores) = count_in_statements(&body.statements);
                        sloads += nested_sloads;
                        sstores += nested_sstores;
                        let (nested_sloads, nested_sstores) = count_in_statements(&post.statements);
                        sloads += nested_sloads;
                        sstores += nested_sstores;
                    }
                    Statement::Block(region) => {
                        let (nested_sloads, nested_sstores) =
                            count_in_statements(&region.statements);
                        sloads += nested_sloads;
                        sstores += nested_sstores;
                    }
                    _ => {}
                }
            }
            (sloads, sstores)
        }

        let (mut total_sloads, mut total_sstores) = count_in_statements(&object.code.statements);
        for function in object.functions.values() {
            let (nested_sloads, nested_sstores) = count_in_statements(&function.body.statements);
            total_sloads += nested_sloads;
            total_sstores += nested_sstores;
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
                    if let Expression::Literal {
                        value: literal_value,
                        ..
                    } = value
                    {
                        literals.insert(bindings[0].0, literal_value.clone());
                    }
                }
            }
        };
        for_each_statement(&object.code.statements, &mut record);
        for function in object.functions.values() {
            for_each_statement(&function.body.statements, &mut record);
        }

        let classify = |value: &Value| -> Option<bool> {
            let literal_value = literals.get(&value.id.0)?;
            if literal_value.is_zero() {
                return None;
            }
            if literal_value.bits() <= 64 {
                return Some(true);
            }
            let low_mask: num::BigUint = (num::BigUint::from(1u32) << 224) - 1u32;
            if (literal_value & &low_mask).is_zero() {
                let top: num::BigUint = literal_value >> 224u32;
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
        let (mapping_sloads, mapping_sstores) = Self::count_mapping_operations(object);
        let combined_mapping_operations = mapping_sloads + mapping_sstores;
        self.use_outlined_mapping_sload =
            mapping_sloads > 0 && combined_mapping_operations >= MAPPING_COMBINED_THRESHOLD;
        self.use_outlined_mapping_sstore =
            mapping_sstores > 0 && combined_mapping_operations >= MAPPING_COMBINED_THRESHOLD;
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
        let noreturn = crate::guard_narrow::detect_noreturn_functions(object);
        for (func_id, function) in &object.functions {
            if let Some(mask) = Self::extract_validator_mask(function, &noreturn) {
                self.validator_masks.insert(func_id.0, mask);
            }
        }

        for (func_id, function) in &object.functions {
            self.declare_function(function, context)?;
            self.function_names.insert(func_id.0, function.name.clone());
            self.function_parameter_types.insert(
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        self.generate_block(&object.code, context)?;

        context
            .set_debug_location(0, 0, None)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        match context
            .basic_block()
            .get_last_instruction()
            .map(|instruction| instruction.get_opcode())
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

        for (index, subobject) in object.subobjects.iter().enumerate() {
            let sub_type_info = self
                .type_info
                .sub_inferences
                .get(index)
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
    ///
    /// Inline hints are a middle-end code-size heuristic; they only apply when
    /// the middle-end optimizer runs. With the middle end disabled (library /
    /// post-link and `-O0` builds) the backend runs `default<O0>`, so inlining
    /// is left to its defaults rather than pinning `AlwaysInline`/`NoInline`.
    fn set_inline_attributes(&self, function: &Function, context: &PolkaVMContext<'ctx>) {
        if !context.optimizer_settings().is_middle_end_enabled() {
            return;
        }

        let declaration = match context.get_function(&function.name, true) {
            Some(func_ref) => func_ref.borrow().declaration(),
            None => return,
        };

        let ir_decision = self.inline_decisions.get(&function.id.0).copied();

        let attribute = match ir_decision {
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

        if let Some(attribute) = attribute {
            revive_llvm_context::PolkaVMFunction::set_attributes(
                context.llvm(),
                declaration,
                &[attribute],
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
                .map(|code_type| format!("{:?}", code_type))
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
        let saved_callvalue_ids = std::mem::take(&mut self.callstored_word_ids);
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
                        .map_err(|error| anyhow::anyhow!("LLVM error: {error}"))?
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
                                    let value_bits = integer_value.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if value_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(integer_value, target, "ret_narrow")
                                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                            .as_basic_value_enum()
                                    } else if value_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(integer_value, target, "ret_widen")
                                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
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
                    .map(|index| match function.returns.get(index) {
                        Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => context
                            .integer_type(bit_width.bits() as usize)
                            .as_basic_type_enum(),
                        _ => context.word_type().as_basic_type_enum(),
                    })
                    .collect();
                let struct_type = context.structure_type(&field_types);
                let mut struct_value = struct_type.get_undef();
                for (index, ret_id) in function.return_values.iter().enumerate() {
                    if let Ok(return_value) = self.get_value(*ret_id) {
                        let return_value = if return_value.is_int_value() {
                            let integer_value = return_value.into_int_value();
                            match function.returns.get(index) {
                                Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => {
                                    let target = context.integer_type(bit_width.bits() as usize);
                                    let value_bits = integer_value.get_type().get_bit_width();
                                    let tgt_bits = target.get_bit_width();
                                    if value_bits > tgt_bits {
                                        context
                                            .builder()
                                            .build_int_truncate(
                                                integer_value,
                                                target,
                                                &format!("ret_narrow_{}", index),
                                            )
                                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                            .as_basic_value_enum()
                                    } else if value_bits < tgt_bits {
                                        context
                                            .builder()
                                            .build_int_z_extend(
                                                integer_value,
                                                target,
                                                &format!("ret_widen_{}", index),
                                            )
                                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                            .as_basic_value_enum()
                                    } else {
                                        integer_value.as_basic_value_enum()
                                    }
                                }
                                _ => self
                                    .ensure_word_type(
                                        context,
                                        integer_value,
                                        &format!("ret_val_{}", index),
                                    )?
                                    .as_basic_value_enum(),
                            }
                        } else {
                            return_value
                        };
                        struct_value = context
                            .builder()
                            .build_insert_value(
                                struct_value,
                                return_value,
                                index as u32,
                                "ret_insert",
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                            .into_struct_value();
                    }
                }
                context.build_store(pointer, struct_value.as_basic_value_enum())?;
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
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                context.build_return(Some(&return_value));
            }
            revive_llvm_context::PolkaVMFunctionReturn::Compound { pointer, .. } => {
                let return_value = context
                    .build_load(pointer, "return_value")
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                context.build_return(Some(&return_value));
            }
        }

        context.pop_debug_scope();

        self.values = saved_values;
        self.callstored_word_ids = saved_callvalue_ids;
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
        let mut index = 0;
        while index < statements.len() {
            if let Some(skip) = self.try_match_return_word(statements, index, context)? {
                index += skip;
                continue;
            }
            self.generate_statement(&statements[index], context)?;
            index += 1;
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
        index: usize,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<Option<usize>> {
        if index >= statements.len() {
            return Ok(None);
        }

        let (store_offset, store_value) = match &statements[index] {
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

        let (ret_offset, ret_length, skip) = if index + 1 < statements.len() {
            if let Statement::Return { offset, length } = &statements[index + 1] {
                (offset, length, 2)
            } else if index + 2 < statements.len() {
                if let Statement::Return { offset, length } = &statements[index + 2] {
                    if let Statement::Let {
                        bindings,
                        value: Expression::Literal { .. },
                    } = &statements[index + 1]
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

        let offset_value = self.translate_value(store_offset)?.into_int_value();
        if Self::try_extract_const_u64(offset_value).is_some() {
            return Ok(None);
        }

        if skip == 3 {
            if let Statement::Let {
                value: Expression::Literal { value, .. },
                ..
            } = &statements[index + 1]
            {
                if value != &num::BigUint::from(revive_common::BYTE_LENGTH_WORD as u64) {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        } else {
            let length_value = self.translate_value(ret_length)?.into_int_value();
            let const_len = Self::try_extract_const_u64(length_value);
            if const_len != Some(revive_common::BYTE_LENGTH_WORD as u64) {
                return Ok(None);
            }
        }

        let offset_narrow = self.narrow_offset_for_pointer(
            context,
            offset_value,
            store_offset.id,
            "return_word_offset_narrow",
        )?;
        let offset_xlen = context
            .safe_truncate_int_to_xlen(offset_narrow)
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let stored_word = self.translate_value(store_value)?.into_int_value();
        let stored_word = self.ensure_word_type(context, stored_word, "return_word_val")?;

        let function = self.get_or_create_return_word_fn(context)?;
        context
            .builder()
            .build_call(function, &[offset_xlen.into(), stored_word.into()], "")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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

        if let Err(error) = self.generate_statement_inner(statement, context) {
            return Err(CodegenError::Llvm(format!(
                "Error in {} statement: {}",
                statement_kind, error
            )));
        }
        Ok(())
    }

    /// Binds each loop variable to its phi node at full width.
    ///
    /// Loop variables are deliberately NOT narrowed. Narrowing the counter (e.g. on
    /// `non_comparison_demand`, which excludes the loop condition) would let a wide-stride counter
    /// wrap at the narrow width while the EVM comparison and increment stay 256-bit. Body sites still
    /// narrow at their own use points.
    fn bind_loop_variables(
        &mut self,
        loop_variables: &[ValueId],
        loop_phis: &[inkwell::values::PhiValue<'ctx>],
    ) {
        for (loop_variable, phi) in loop_variables.iter().zip(loop_phis.iter()) {
            self.set_value(*loop_variable, phi.as_basic_value());
        }
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
                    self.callstored_word_ids.insert(bindings[0].0);
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
                    let struct_value = llvm_value.into_struct_value();
                    for (index, binding) in bindings.iter().enumerate() {
                        let field = context
                            .builder()
                            .build_extract_value(struct_value, index as u32, &format!("{}", index))
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
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
                let offset_value = self.translate_value(offset)?.into_int_value();
                let stored_word = self.translate_value(value)?.into_int_value();
                let stored_word = self.ensure_word_type(context, stored_word, "mstore_val")?;

                match self.native_memory_mode(context, offset_value) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mstore_offset_xlen",
                        )?;
                        context.build_store_native(offset_xlen, stored_word)?;
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mstore_offset_xlen",
                        )?;
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        let is_fmp_store = Self::try_extract_const_u64(offset_value)
                            == Some(FREE_MEMORY_POINTER_SLOT)
                            && matches!(region, MemoryRegion::FreePointerSlot)
                            && !self.heap_opt.fmp_could_be_unbounded();
                        let store_value: inkwell::values::BasicValueEnum = if is_fmp_store {
                            context
                                .builder()
                                .build_int_truncate(
                                    stored_word,
                                    context.xlen_type(),
                                    "fmp_store_trunc",
                                )
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                .into()
                        } else {
                            stored_word.into()
                        };
                        context
                            .builder()
                            .build_store(pointer.value, store_value)
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                            .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
                            .expect("Alignment is valid");
                        self.advance_msize_watermark(context, offset_value)?;
                    }
                    NativeMemoryMode::InlineByteSwap => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mstore_offset_xlen",
                        )?;
                        if stored_word.is_const() {
                            revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                                context,
                                offset_xlen,
                                stored_word,
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        } else {
                            let function = self.get_or_create_store_bswap_fn(context)?;
                            let value_word =
                                self.ensure_word_type(context, stored_word, "store_bswap_val")?;
                            context
                                .builder()
                                .build_call(
                                    function,
                                    &[offset_xlen.into(), value_word.into()],
                                    "store_bswap",
                                )
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        }
                        self.advance_msize_watermark(context, offset_value)?;
                    }
                    NativeMemoryMode::ByteSwap => {
                        let offset_value = self.narrow_offset_for_pointer(
                            context,
                            offset_value,
                            offset.id,
                            "mstore_offset_narrow",
                        )?;
                        if !self.has_msize {
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_value)
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            if stored_word.is_null() {
                                let function = self.get_or_create_store_zero_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(function, &[offset_xlen.into()], "")
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            } else if self.use_outlined_store_low_word
                                && Self::value_fits_in_i64(stored_word).is_some()
                            {
                                let low = Self::value_fits_in_i64(stored_word).unwrap();
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
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            } else if self.use_outlined_store_high_word
                                && Self::value_is_selector_shl_224(stored_word).is_some()
                            {
                                let selector =
                                    Self::value_is_selector_shl_224(stored_word).unwrap();
                                let function =
                                    self.get_or_create_store_high_word_checked_fn(context)?;
                                let selector_const =
                                    context.llvm().i32_type().const_int(selector as u64, false);
                                context
                                    .builder()
                                    .build_call(
                                        function,
                                        &[offset_xlen.into(), selector_const.into()],
                                        "",
                                    )
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            } else {
                                let function =
                                    self.get_or_create_store_bswap_checked_fn(context)?;
                                context
                                    .builder()
                                    .build_call(
                                        function,
                                        &[offset_xlen.into(), stored_word.into()],
                                        "",
                                    )
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            }
                        } else {
                            revive_llvm_context::polkavm_evm_memory::store(
                                context,
                                offset_value,
                                stored_word,
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
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "mstore8_offset_narrow",
                )?;
                let stored_word = self.translate_value(value)?.into_int_value();
                let stored_word = self.ensure_word_type(context, stored_word, "mstore8_val")?;
                revive_llvm_context::polkavm_evm_memory::store_byte(
                    context,
                    offset_value,
                    stored_word,
                )?;
            }

            Statement::MCopy {
                destination,
                source,
                length,
            } => {
                let destination_value = self.translate_value(destination)?.into_int_value();
                let destination_value = self.narrow_offset_for_pointer(
                    context,
                    destination_value,
                    destination.id,
                    "mcopy_dest_narrow",
                )?;
                let source_value = self.translate_value(source)?.into_int_value();
                let source_value = self.narrow_offset_for_pointer(
                    context,
                    source_value,
                    source.id,
                    "mcopy_src_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "mcopy_length_narrow",
                )?;

                let destination_pointer = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    destination_value,
                    "mcopy_destination",
                );
                let source_pointer = revive_llvm_context::PolkaVMPointer::new_with_offset(
                    context,
                    revive_llvm_context::PolkaVMAddressSpace::Heap,
                    context.byte_type(),
                    source_value,
                    "mcopy_source",
                );

                context.build_memcpy(
                    destination_pointer,
                    source_pointer,
                    length_value,
                    "mcopy_size",
                )?;
            }

            Statement::SStore {
                key,
                value,
                static_slot: _,
            } => {
                let key_argument = self.value_to_storage_key_argument(key, context)?;
                if key_argument.is_register() {
                    let key_value = key_argument
                        .access(context)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let stored_word = self
                        .value_to_argument(value, context)?
                        .access(context)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let sstore_fn = self.get_or_create_sstore_word_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            sstore_fn,
                            &[key_value.into(), stored_word.into()],
                            "sstore_word",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                } else {
                    let value_argument = self.value_to_argument(value, context)?;
                    revive_llvm_context::polkavm_evm_storage::store(
                        context,
                        &key_argument,
                        &value_argument,
                    )?;
                }
            }

            Statement::TStore { key, value } => {
                let key_argument = self.value_to_argument(key, context)?;
                let value_argument = self.value_to_argument(value, context)?;
                revive_llvm_context::polkavm_evm_storage::transient_store(
                    context,
                    &key_argument,
                    &value_argument,
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
                    && self.callstored_word_ids.contains(&condition.id.0)
                    && else_region.is_none()
                    && outputs.is_empty()
                    && Self::is_revert_zero_region(then_region)
                {
                    let function = self.get_or_create_callvalue_check_fn(context)?;
                    context
                        .builder()
                        .build_call(function, &[], "callvalue_check")
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    return Ok(());
                }

                let condition_bool = if self.use_outlined_callvalue
                    && self.callstored_word_ids.contains(&condition.id.0)
                {
                    revive_llvm_context::polkavm_evm_ether_gas::value_nonzero_outlined(context)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                        .into_int_value()
                } else {
                    let condition_value = self.translate_value(condition)?.into_int_value();
                    let condition_zero = condition_value.get_type().const_zero();
                    context
                        .builder()
                        .build_int_compare(
                            inkwell::IntPredicate::NE,
                            condition_value,
                            condition_zero,
                            "cond_bool",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                };

                let then_block = context.append_basic_block("if_then");
                let join_block = context.append_basic_block("if_join");

                let mut phi_incoming: Vec<(
                    Vec<BasicValueEnum<'ctx>>,
                    inkwell::basic_block::BasicBlock<'ctx>,
                )> = Vec::new();

                if let Some(else_region) = else_region {
                    let else_block = context.append_basic_block("if_else");
                    context.build_conditional_branch(condition_bool, then_block, else_block)?;

                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    let then_end_block = context.basic_block();
                    if !Self::block_is_unreachable(then_end_block) {
                        let mut then_yields = Vec::new();
                        for (index, yield_value) in then_region.yields.iter().enumerate() {
                            then_yields.push(self.translate_value_as_word(
                                yield_value,
                                context,
                                &format!("then_yield_{}", index),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((then_yields, then_end_block));
                    } else if then_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }

                    context.set_basic_block(else_block);
                    self.generate_region(else_region, context)?;
                    let else_end_block = context.basic_block();
                    if !Self::block_is_unreachable(else_end_block) {
                        let mut else_yields = Vec::new();
                        for (index, yield_value) in else_region.yields.iter().enumerate() {
                            else_yields.push(self.translate_value_as_word(
                                yield_value,
                                context,
                                &format!("else_yield_{}", index),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((else_yields, else_end_block));
                    } else if else_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }
                } else {
                    let entry_block = context.basic_block();
                    context.build_conditional_branch(condition_bool, then_block, join_block)?;

                    let mut else_yields = Vec::new();
                    for (index, input_value) in inputs.iter().enumerate() {
                        else_yields.push(self.translate_value_as_word(
                            input_value,
                            context,
                            &format!("input_yield_{}", index),
                        )?);
                    }
                    phi_incoming.push((else_yields, entry_block));

                    context.set_basic_block(then_block);
                    self.generate_region(then_region, context)?;
                    let then_end_block = context.basic_block();
                    if !Self::block_is_unreachable(then_end_block) {
                        let mut then_yields = Vec::new();
                        for (index, yield_value) in then_region.yields.iter().enumerate() {
                            then_yields.push(self.translate_value_as_word(
                                yield_value,
                                context,
                                &format!("then_yield_{}", index),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        phi_incoming.push((then_yields, then_end_block));
                    } else if then_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                    for (index, output_id) in outputs.iter().enumerate() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("if_phi_{}", index))
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        for (yields, block) in &phi_incoming {
                            phi.add_incoming(&[(&yields[index], *block)]);
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
                    for (index, output_id) in outputs.iter().enumerate() {
                        self.set_value(*output_id, yields[index]);
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
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                let mut scrutinee_value = self.translate_value(scrutinee)?.into_int_value();
                let scrutinee_width = scrutinee_value.get_type().get_bit_width();

                if scrutinee_width > 32 {
                    let all_cases_fit_32 = cases.iter().all(|case| {
                        case.value
                            .to_u64()
                            .is_some_and(|value| value <= u32::MAX as u64)
                    });
                    if all_cases_fit_32 {
                        let provable =
                            Self::provable_narrow_width(scrutinee_value).unwrap_or(scrutinee_width);
                        if provable <= 32 {
                            scrutinee_value = context
                                .builder()
                                .build_int_truncate(
                                    scrutinee_value,
                                    context.llvm().i32_type(),
                                    "switch_narrow",
                                )
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        }
                    }
                }

                scrutinee_value =
                    self.widen_scrutinee_for_case_labels(context, scrutinee_value, cases)?;

                let scrut_type = scrutinee_value.get_type();
                let join_block = context.append_basic_block("switch_join");

                let mut case_blocks = Vec::new();
                for (index, case) in cases.iter().enumerate() {
                    let case_block = context.append_basic_block(&format!("switch_case_{}", index));
                    let digits = case.value.to_u64_digits();
                    let case_value = if digits.is_empty() {
                        scrut_type.const_zero()
                    } else {
                        scrut_type.const_int_arbitrary_precision(&digits)
                    };
                    case_blocks.push((case_value, case_block, &case.body));
                }

                let default_block = context.append_basic_block("switch_default");

                let switch_cases: Vec<_> = case_blocks
                    .iter()
                    .map(|(value, block, _)| (*value, *block))
                    .collect();
                context
                    .builder()
                    .build_switch(scrutinee_value, default_block, &switch_cases)
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;

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
                        for (yield_index, yield_value) in body.yields.iter().enumerate() {
                            match self.translate_value_as_word(
                                yield_value,
                                context,
                                &format!("case_{}_yield_{}", index, yield_index),
                            ) {
                                Ok(word) => yields.push(word),
                                Err(error) => {
                                    return Err(CodegenError::Llvm(format!(
                                        "Switch case {} yield {}: {:?} - {}",
                                        index, yield_index, yield_value.id, error
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
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }
                }

                context.set_basic_block(default_block);
                if let Some(default_region) = default {
                    self.generate_region(default_region, context)?;
                    let default_end_block = context.basic_block();

                    if !Self::block_is_unreachable(default_end_block) {
                        let mut default_yields = Vec::new();
                        for (index, yield_value) in default_region.yields.iter().enumerate() {
                            default_yields.push(self.translate_value_as_word(
                                yield_value,
                                context,
                                &format!("default_yield_{}", index),
                            )?);
                        }
                        context.build_unconditional_branch(join_block);
                        all_yields.push((default_yields, default_end_block));
                    } else if default_end_block.get_terminator().is_none() {
                        context
                            .builder()
                            .build_unreachable()
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }
                } else {
                    let default_end_block = context.basic_block();
                    let mut default_yields = Vec::new();
                    for (index, input_value) in inputs.iter().enumerate() {
                        default_yields.push(self.translate_value_as_word(
                            input_value,
                            context,
                            &format!("default_input_{}", index),
                        )?);
                    }
                    context.build_unconditional_branch(join_block);
                    all_yields.push((default_yields, default_end_block));
                }

                context.set_basic_block(join_block);

                if all_yields.len() >= 2 {
                    for (index, output_id) in outputs.iter().enumerate() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("switch_phi_{}", index))
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                        for (yields, end_block) in &all_yields {
                            if index < yields.len() {
                                phi.add_incoming(&[(&yields[index], *end_block)]);
                            }
                        }
                        self.set_value(*output_id, phi.as_basic_value());
                    }
                } else if all_yields.len() == 1 {
                    let (yields, _) = &all_yields[0];
                    for (index, output_id) in outputs.iter().enumerate() {
                        if index < yields.len() {
                            self.set_value(*output_id, yields[index]);
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
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                let mut initial_llvm_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (index, initial_value) in initial_values.iter().enumerate() {
                    initial_llvm_values.push(self.translate_value_as_word(
                        initial_value,
                        context,
                        &format!("for_init_{}", index),
                    )?);
                }

                let entry_block = context.basic_block();
                let condition_block = context.append_basic_block("for_cond");
                let body_block = context.append_basic_block("for_body");
                let continue_landing = context.append_basic_block("for_continue_landing");
                let post_block = context.append_basic_block("for_post");
                let join_block = context.append_basic_block("for_join");

                context.build_unconditional_branch(condition_block);
                context.set_basic_block(condition_block);

                let mut loop_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let mut loop_phi_values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (index, _loop_variable) in loop_variables.iter().enumerate() {
                    let phi = context
                        .builder()
                        .build_phi(context.word_type(), &format!("loop_var_{}", index))
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                    if index < initial_llvm_values.len() {
                        phi.add_incoming(&[(&initial_llvm_values[index], entry_block)]);
                    }

                    loop_phi_values.push(phi.as_basic_value());
                    loop_phis.push(phi);
                }

                self.bind_loop_variables(loop_variables, &loop_phis);

                for statement in condition_statements {
                    self.generate_statement(statement, context)?;
                }

                let condition_value = self
                    .generate_expression(condition, context, None)?
                    .into_int_value();
                let condition_zero = condition_value.get_type().const_zero();
                let condition_bool = context
                    .builder()
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        condition_value,
                        condition_zero,
                        "for_cond_bool",
                    )
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                let condition_eval_block = context.basic_block();

                context.set_basic_block(join_block);
                let mut join_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let has_loop_variables = !loop_variables.is_empty();
                if has_loop_variables {
                    for index in 0..loop_variables.len() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("join_phi_{}", index))
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        join_phis.push(phi);
                    }
                }

                context.set_basic_block(condition_eval_block);
                context.build_conditional_branch(condition_bool, body_block, join_block)?;
                if has_loop_variables {
                    for (index, phi) in join_phis.iter().enumerate() {
                        phi.add_incoming(&[(&loop_phi_values[index], condition_eval_block)]);
                    }
                }

                context.set_basic_block(continue_landing);
                let mut landing_phis: Vec<inkwell::values::PhiValue<'ctx>> = Vec::new();
                let has_body_yields = !body.yields.is_empty();
                if has_body_yields {
                    for index in 0..body.yields.len() {
                        let phi = context
                            .builder()
                            .build_phi(context.word_type(), &format!("continue_landing_{}", index))
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        landing_phis.push(phi);
                    }
                }
                context.build_unconditional_branch(post_block);

                context.push_loop(body_block, continue_landing, join_block);

                self.for_loop_post_phis.push(ForLoopPostPhis {
                    phis: landing_phis.clone(),
                    loop_variable_phi_values: loop_phi_values.clone(),
                });

                self.for_loop_break_phis.push(ForLoopBreakPhis {
                    phis: join_phis.clone(),
                    loop_variable_phi_values: loop_phi_values.clone(),
                });

                context.set_basic_block(body_block);
                self.generate_region(body, context)?;

                let body_end_block = context.basic_block();
                let mut body_yield_values: Vec<inkwell::values::BasicValueEnum<'ctx>> = Vec::new();
                if has_body_yields {
                    for (index, yield_ref) in body.yields.iter().enumerate() {
                        let yield_value = self.translate_value_as_word(
                            yield_ref,
                            context,
                            &format!("body_yield_{}", index),
                        )?;
                        body_yield_values.push(yield_value.as_basic_value_enum());
                    }
                }

                context.build_unconditional_branch(continue_landing);

                for (phi, yield_value) in landing_phis.iter().zip(body_yield_values.iter()) {
                    phi.add_incoming(&[(yield_value, body_end_block)]);
                }

                self.for_loop_post_phis.pop();
                self.for_loop_break_phis.pop();

                context.set_basic_block(post_block);

                if has_body_yields {
                    for (index, phi) in landing_phis.iter().enumerate() {
                        if index < post_input_variables.len() {
                            self.set_value(post_input_variables[index], phi.as_basic_value());
                        }
                    }
                }

                self.generate_region(post, context)?;

                let post_end_block = context.basic_block();
                for (index, phi) in loop_phis.iter().enumerate() {
                    if index < post.yields.len() {
                        let yield_value = self.translate_value_as_word(
                            &post.yields[index],
                            context,
                            &format!("for_post_yield_{}", index),
                        )?;
                        phi.add_incoming(&[(&yield_value, post_end_block)]);
                    }
                }

                context.build_unconditional_branch(condition_block);

                context.pop_loop();
                context.set_basic_block(join_block);

                if has_loop_variables {
                    for (index, output_id) in outputs.iter().enumerate() {
                        if index < join_phis.len() {
                            self.set_value(*output_id, join_phis[index].as_basic_value());
                        }
                    }
                } else {
                    for (index, output_id) in outputs.iter().enumerate() {
                        if index < loop_phis.len() {
                            self.set_value(*output_id, loop_phis[index].as_basic_value());
                        }
                    }
                }
            }

            Statement::Break { values } => {
                if let Some(break_phis) = self.for_loop_break_phis.last() {
                    let current_block = context.basic_block();
                    for (index, phi) in break_phis.phis.iter().enumerate() {
                        let value = if index < values.len() {
                            self.translate_value_as_word(
                                &values[index],
                                context,
                                &format!("break_val_{}", index),
                            )?
                            .as_basic_value_enum()
                        } else {
                            break_phis.loop_variable_phi_values[index]
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
                    for (index, phi) in post_phis.phis.iter().enumerate() {
                        let value = if index < values.len() {
                            self.translate_value_as_word(
                                &values[index],
                                context,
                                &format!("continue_val_{}", index),
                            )?
                            .as_basic_value_enum()
                        } else {
                            post_phis.loop_variable_phi_values[index]
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
                                            let value_bits =
                                                integer_value.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if value_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        integer_value,
                                                        target,
                                                        "leave_narrow",
                                                    )
                                                    .map_err(|error| {
                                                        CodegenError::Llvm(error.to_string())
                                                    })?
                                                    .as_basic_value_enum()
                                            } else if value_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        integer_value,
                                                        target,
                                                        "leave_widen",
                                                    )
                                                    .map_err(|error| {
                                                        CodegenError::Llvm(error.to_string())
                                                    })?
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
                            .map(|index| match self.current_return_types.get(index) {
                                Some(Type::Int(bit_width)) if *bit_width < BitWidth::I256 => {
                                    context
                                        .integer_type(bit_width.bits() as usize)
                                        .as_basic_type_enum()
                                }
                                _ => context.word_type().as_basic_type_enum(),
                            })
                            .collect();
                        let struct_type = context.structure_type(&field_types);
                        let mut struct_value = struct_type.get_undef();
                        for (index, return_value) in return_values.iter().enumerate() {
                            if let Ok(value) = self.translate_value(return_value) {
                                let value = if value.is_int_value() {
                                    let integer_value = value.into_int_value();
                                    match self.current_return_types.get(index) {
                                        Some(Type::Int(bit_width))
                                            if *bit_width < BitWidth::I256 =>
                                        {
                                            let target =
                                                context.integer_type(bit_width.bits() as usize);
                                            let value_bits =
                                                integer_value.get_type().get_bit_width();
                                            let tgt_bits = target.get_bit_width();
                                            if value_bits > tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_truncate(
                                                        integer_value,
                                                        target,
                                                        &format!("leave_narrow_{}", index),
                                                    )
                                                    .map_err(|error| {
                                                        CodegenError::Llvm(error.to_string())
                                                    })?
                                                    .as_basic_value_enum()
                                            } else if value_bits < tgt_bits {
                                                context
                                                    .builder()
                                                    .build_int_z_extend(
                                                        integer_value,
                                                        target,
                                                        &format!("leave_widen_{}", index),
                                                    )
                                                    .map_err(|error| {
                                                        CodegenError::Llvm(error.to_string())
                                                    })?
                                                    .as_basic_value_enum()
                                            } else {
                                                integer_value.as_basic_value_enum()
                                            }
                                        }
                                        _ => self
                                            .ensure_word_type(
                                                context,
                                                integer_value,
                                                &format!("leave_ret_val_{}", index),
                                            )?
                                            .as_basic_value_enum(),
                                    }
                                } else {
                                    value
                                };
                                struct_value = context
                                    .builder()
                                    .build_insert_value(
                                        struct_value,
                                        value,
                                        index as u32,
                                        "ret_insert",
                                    )
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                    .into_struct_value();
                            }
                        }
                        context.build_store(pointer, struct_value.as_basic_value_enum())?;
                    }
                }
                let return_block = context.current_function().borrow().return_block();
                context.build_unconditional_branch(return_block);
                let unreachable = context.append_basic_block("leave_unreachable");
                context.set_basic_block(unreachable);
            }

            Statement::Revert { offset, length } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                let length_value = self.translate_value(length)?.into_int_value();

                if Self::is_const_zero(offset_value) {
                    if let Some(const_len) = Self::try_extract_const_u64(length_value) {
                        let revert_block = self.get_or_create_revert_block(context, const_len)?;
                        context.build_unconditional_branch(revert_block);
                        let dead_block = context.append_basic_block("revert_dedup_dead");
                        context.set_basic_block(dead_block);
                        return Ok(());
                    }
                }
                if !self.has_msize {
                    let offset_xlen = context
                        .safe_truncate_int_to_xlen(offset_value)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_value)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let flags = context.xlen_type().const_int(1, false);
                    let function = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            function,
                            &[flags.into(), offset_xlen.into(), length_xlen.into()],
                            "",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                } else {
                    let offset_value =
                        self.ensure_word_type(context, offset_value, "revert_offset")?;
                    let length_value =
                        self.ensure_word_type(context, length_value, "revert_length")?;
                    revive_llvm_context::polkavm_evm_return::revert(
                        context,
                        offset_value,
                        length_value,
                    )?;
                }
            }

            Statement::Return { offset, length } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                let length_value = self.translate_value(length)?.into_int_value();

                if let (Some(const_offset), Some(const_len)) = (
                    Self::try_extract_const_u64(offset_value),
                    Self::try_extract_const_u64(length_value),
                ) {
                    let return_block =
                        self.get_or_create_return_block(context, const_offset, const_len)?;
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
                        .safe_truncate_int_to_xlen(offset_value)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let length_xlen = context
                        .safe_truncate_int_to_xlen(length_value)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let flags = context.xlen_type().const_int(0, false);
                    let function = self.get_or_create_exit_checked_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            function,
                            &[flags.into(), offset_xlen.into(), length_xlen.into()],
                            "",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                } else {
                    let offset_value =
                        self.ensure_word_type(context, offset_value, "return_offset")?;
                    let length_value =
                        self.ensure_word_type(context, length_value, "return_length")?;
                    revive_llvm_context::polkavm_evm_return::r#return(
                        context,
                        offset_value,
                        length_value,
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

                    let length_value = context.word_const(*length as u64);
                    let mut arguments: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![length_value.into()];
                    for word in data {
                        let word_value = context.word_const_str_hex(&word.to_str_radix(16));
                        arguments.push(word_value.into());
                    }

                    context
                        .builder()
                        .build_call(function, &arguments, "error_string_revert")
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    context.build_unreachable();
                } else {
                    let fmp_offset = context.word_const(FREE_MEMORY_POINTER_SLOT);
                    let fmp = revive_llvm_context::polkavm_evm_memory::load(context, fmp_offset)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                        .into_int_value();

                    let error_selector =
                        context.word_const_str_hex(revive_common::ERROR_STRING_SELECTOR_WORD_HEX);
                    revive_llvm_context::polkavm_evm_memory::store(context, fmp, error_selector)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                    let fmp_plus_offset_field = context
                        .builder()
                        .build_int_add(
                            fmp,
                            context.word_const(ABI_SELECTOR_LENGTH),
                            "fmp_offset_field",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_plus_offset_field,
                        context.word_const(revive_common::BYTE_LENGTH_WORD as u64),
                    )
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                    let fmp_plus_length_field = context
                        .builder()
                        .build_int_add(
                            fmp,
                            context.word_const(ERROR_STRING_LENGTH_FIELD_OFFSET),
                            "fmp_length_field",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    revive_llvm_context::polkavm_evm_memory::store(
                        context,
                        fmp_plus_length_field,
                        context.word_const(*length as u64),
                    )
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                    for (index, word) in data.iter().enumerate() {
                        let offset = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                            + (index as u64) * revive_common::BYTE_LENGTH_WORD as u64;
                        let fmp_plus_offset = context
                            .builder()
                            .build_int_add(
                                fmp,
                                context.word_const(offset),
                                &format!("fmp_{offset:x}"),
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        let word_value = context.word_const_str_hex(&word.to_str_radix(16));
                        revive_llvm_context::polkavm_evm_memory::store(
                            context,
                            fmp_plus_offset,
                            word_value,
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }

                    let total_length = ERROR_STRING_FIRST_DATA_WORD_OFFSET
                        + (num_words as u64) * revive_common::BYTE_LENGTH_WORD as u64;
                    revive_llvm_context::polkavm_evm_return::revert(
                        context,
                        fmp,
                        context.word_const(total_length),
                    )
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    context.build_unreachable();
                }

                let dead_block = context.append_basic_block("error_string_dead");
                context.set_basic_block(dead_block);
            }

            Statement::CustomErrorRevert {
                selector,
                arguments,
            } => {
                let num_arguments = arguments.len();
                let count = self
                    .custom_error_revert_counts
                    .get(&num_arguments)
                    .copied()
                    .unwrap_or(0);

                if count >= 3 {
                    let function =
                        self.get_or_create_custom_error_revert_fn(num_arguments, context)?;

                    let selector_high32 = (selector >> 224u32)
                        .iter_u32_digits()
                        .next()
                        .unwrap_or(0)
                        .swap_bytes();
                    let selector_value =
                        context.xlen_type().const_int(selector_high32 as u64, false);
                    let mut call_arguments: Vec<inkwell::values::BasicMetadataValueEnum> =
                        vec![selector_value.into()];
                    for argument in arguments {
                        let argument_value = self.translate_value(argument)?.into_int_value();
                        let argument_value =
                            self.ensure_word_type(context, argument_value, "custom_error_arg")?;
                        call_arguments.push(argument_value.into());
                    }

                    context
                        .builder()
                        .build_call(function, &call_arguments, "custom_error_revert")
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    context.build_unreachable();
                } else {
                    let selector_value = context.word_const_str_hex(&selector.to_str_radix(16));
                    let offset_0 = context.xlen_type().const_int(0, false);
                    revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                        context,
                        offset_0,
                        selector_value,
                    )
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;

                    for (index, argument) in arguments.iter().enumerate() {
                        let argument_value = self.translate_value(argument)?.into_int_value();
                        let argument_value =
                            self.ensure_word_type(context, argument_value, "custom_error_arg")?;
                        let byte_offset = 4 + (index as u64) * 0x20;
                        let offset_value = context.xlen_type().const_int(byte_offset, false);
                        revive_llvm_context::polkavm_evm_memory::store_bswap_unchecked(
                            context,
                            offset_value,
                            argument_value,
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    }

                    let const_len = 4 + (num_arguments as u64) * 0x20;
                    let revert_block = self.get_or_create_revert_block(context, const_len)?;
                    context.build_unconditional_branch(revert_block);
                }

                let dead_block = context.append_basic_block("custom_error_dead");
                context.set_basic_block(dead_block);
            }

            Statement::SelfDestruct { address } => {
                let address_value = self.translate_value(address)?.into_int_value();
                let address_value =
                    self.ensure_word_type(context, address_value, "selfdestruct_addr")?;
                revive_llvm_context::polkavm_evm_return::selfdestruct(context, address_value)?;
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

                let gas_value = self.translate_value(gas)?.into_int_value();
                let gas_value = self.ensure_word_type(context, gas_value, "call_gas")?;
                let address_value = self.translate_value(address)?.into_int_value();
                let address_value = self.ensure_word_type(context, address_value, "call_addr")?;
                let arguments_offset_value = self.translate_value(args_offset)?.into_int_value();
                let arguments_offset_value = self.narrow_offset_for_pointer(
                    context,
                    arguments_offset_value,
                    args_offset.id,
                    "call_args_offset_narrow",
                )?;
                let arguments_length_value = self.translate_value(args_length)?.into_int_value();
                let arguments_length_value = self.narrow_offset_for_pointer(
                    context,
                    arguments_length_value,
                    args_length.id,
                    "call_args_length_narrow",
                )?;
                let ret_offset_value = self.translate_value(ret_offset)?.into_int_value();
                let ret_offset_value = self.narrow_offset_for_pointer(
                    context,
                    ret_offset_value,
                    ret_offset.id,
                    "call_ret_offset_narrow",
                )?;
                let ret_length_value = self.translate_value(ret_length)?.into_int_value();
                let ret_length_value = self.narrow_offset_for_pointer(
                    context,
                    ret_length_value,
                    ret_length.id,
                    "call_ret_length_narrow",
                )?;

                let call_result = match kind {
                    CallKind::Call => {
                        let stored_word = value
                            .map(|operand| -> Result<_> {
                                let value = self.translate_value(&operand)?.into_int_value();
                                self.ensure_word_type(context, value, "call_value")
                            })
                            .transpose()?;
                        revive_llvm_context::polkavm_evm_call::call(
                            context,
                            gas_value,
                            address_value,
                            stored_word,
                            arguments_offset_value,
                            arguments_length_value,
                            ret_offset_value,
                            ret_length_value,
                            vec![],
                            false,
                        )?
                    }
                    CallKind::CallCode => {
                        unreachable!("CallCode is handled above")
                    }
                    CallKind::StaticCall => revive_llvm_context::polkavm_evm_call::call(
                        context,
                        gas_value,
                        address_value,
                        None,
                        arguments_offset_value,
                        arguments_length_value,
                        ret_offset_value,
                        ret_length_value,
                        vec![],
                        true,
                    )?,
                    CallKind::DelegateCall => revive_llvm_context::polkavm_evm_call::delegate_call(
                        context,
                        gas_value,
                        address_value,
                        arguments_offset_value,
                        arguments_length_value,
                        ret_offset_value,
                        ret_length_value,
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
                let stored_word = self.translate_value(value)?.into_int_value();
                let stored_word = self.ensure_word_type(context, stored_word, "create_value")?;
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "create_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "create_length_narrow",
                )?;
                let salt_value = match (kind, salt) {
                    (CreateKind::Create2, Some(salt_argument)) => {
                        let salt_argument_value =
                            self.translate_value(salt_argument)?.into_int_value();
                        Some(self.ensure_word_type(context, salt_argument_value, "create_salt")?)
                    }
                    _ => None,
                };

                let create_result = revive_llvm_context::polkavm_evm_create::create(
                    context,
                    stored_word,
                    offset_value,
                    length_value,
                    salt_value,
                )?;
                self.set_value(*result, create_result);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "log_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "log_length_narrow",
                )?;
                let topic_values: Vec<BasicValueEnum<'ctx>> = topics
                    .iter()
                    .enumerate()
                    .map(|(index, topic)| {
                        let value = self.translate_value(topic)?.into_int_value();
                        let value =
                            self.ensure_word_type(context, value, &format!("log_topic_{}", index))?;
                        Ok(value.as_basic_value_enum())
                    })
                    .collect::<Result<_>>()?;

                {
                    match topic_values.len() {
                        0 => revive_llvm_context::polkavm_evm_event::log::<0>(
                            context,
                            offset_value,
                            length_value,
                            [],
                        )?,
                        1 => revive_llvm_context::polkavm_evm_event::log::<1>(
                            context,
                            offset_value,
                            length_value,
                            [topic_values[0]],
                        )?,
                        2 => revive_llvm_context::polkavm_evm_event::log::<2>(
                            context,
                            offset_value,
                            length_value,
                            [topic_values[0], topic_values[1]],
                        )?,
                        3 => revive_llvm_context::polkavm_evm_event::log::<3>(
                            context,
                            offset_value,
                            length_value,
                            [topic_values[0], topic_values[1], topic_values[2]],
                        )?,
                        4 => revive_llvm_context::polkavm_evm_event::log::<4>(
                            context,
                            offset_value,
                            length_value,
                            [
                                topic_values[0],
                                topic_values[1],
                                topic_values[2],
                                topic_values[3],
                            ],
                        )?,
                        _ => return Err(CodegenError::Unsupported("log with >4 topics".into())),
                    }
                }
            }

            Statement::CodeCopy {
                destination,
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
                let destination_value = self.translate_value(destination)?.into_int_value();
                let destination_value = self.narrow_offset_for_pointer(
                    context,
                    destination_value,
                    destination.id,
                    "codecopy_dest_narrow",
                )?;
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "codecopy_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "codecopy_length_narrow",
                )?;
                revive_llvm_context::polkavm_evm_calldata::copy(
                    context,
                    destination_value,
                    offset_value,
                    length_value,
                )?;
            }

            Statement::ExtCodeCopy { .. } => {
                return Err(CodegenError::Unsupported(
                    "The `EXTCODECOPY` instruction is not supported".into(),
                ));
            }

            Statement::ReturnDataCopy {
                destination,
                offset,
                length,
            } => {
                let destination_value = self.translate_value(destination)?.into_int_value();
                let destination_value = self.narrow_offset_for_pointer(
                    context,
                    destination_value,
                    destination.id,
                    "returndatacopy_dest_narrow",
                )?;
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "returndatacopy_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "returndatacopy_length_narrow",
                )?;
                revive_llvm_context::polkavm_evm_return_data::copy(
                    context,
                    destination_value,
                    offset_value,
                    length_value,
                )?;
            }

            Statement::DataCopy {
                destination,
                offset,
                length: _,
            } => {
                let destination_value = self.translate_value(destination)?.into_int_value();
                let destination_value = self.narrow_offset_for_pointer(
                    context,
                    destination_value,
                    destination.id,
                    "datacopy_dest_narrow",
                )?;
                let hash_value = self.translate_value(offset)?.into_int_value();
                let hash_value = self.ensure_word_type(context, hash_value, "datacopy_hash")?;
                revive_llvm_context::polkavm_evm_memory::store(
                    context,
                    destination_value,
                    hash_value,
                )?;
            }

            Statement::CallDataCopy {
                destination,
                offset,
                length,
            } => {
                let destination_value = self.translate_value(destination)?.into_int_value();
                let destination_value = self.narrow_offset_for_pointer(
                    context,
                    destination_value,
                    destination.id,
                    "calldatacopy_dest_narrow",
                )?;
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "calldatacopy_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "calldatacopy_length_narrow",
                )?;
                revive_llvm_context::polkavm_evm_calldata::copy(
                    context,
                    destination_value,
                    offset_value,
                    length_value,
                )?;
            }

            Statement::Block(region) => {
                self.generate_region(region, context)?;
            }

            Statement::Expression(expression) => {
                let _ = self.generate_expression(expression, context, None)?;
            }

            Statement::MappingSStore { key, slot, value } => {
                let key_value = self.translate_value(key)?.into_int_value();
                let key_value = self.ensure_word_type(context, key_value, "mapping_sstore_key")?;
                let slot_value = self.translate_value(slot)?.into_int_value();
                let slot_value =
                    self.ensure_word_type(context, slot_value, "mapping_sstore_slot")?;
                let stored_word = self.translate_value(value)?.into_int_value();
                let stored_word =
                    self.ensure_word_type(context, stored_word, "mapping_sstore_value")?;

                if self.use_outlined_mapping_sstore {
                    let mapping_sstore_fn = self.get_or_create_mapping_sstore_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            mapping_sstore_fn,
                            &[key_value.into(), slot_value.into(), stored_word.into()],
                            "mapping_sstore_call",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                } else {
                    let hash_value = if slot_value.is_const()
                        && Self::try_extract_const_u64(slot_value).is_none()
                    {
                        let wrapper_fn =
                            self.get_or_create_keccak256_slot_wrapper(slot_value, context)?;
                        let function_type = context
                            .word_type()
                            .fn_type(&[context.word_type().into()], false);
                        context
                            .builder()
                            .build_indirect_call(
                                function_type,
                                wrapper_fn.as_global_value().as_pointer_value(),
                                &[key_value.into()],
                                "keccak256_slot_call",
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                            .try_as_basic_value()
                            .basic()
                            .expect("keccak256 slot wrapper should return a value")
                            .into_int_value()
                    } else {
                        revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                            context, key_value, slot_value,
                        )?
                        .into_int_value()
                    };
                    let sstore_fn = self.get_or_create_sstore_word_fn(context)?;
                    context
                        .builder()
                        .build_call(
                            sstore_fn,
                            &[hash_value.into(), stored_word.into()],
                            "mapping_sstore_word",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                    let value_u64 = value.to_u64().unwrap_or(0);
                    Ok(context
                        .llvm()
                        .i64_type()
                        .const_int(value_u64, false)
                        .as_basic_value_enum())
                } else {
                    let value_str = value.to_string();
                    Ok(context.word_const_str_dec(&value_str).as_basic_value_enum())
                }
            }

            Expression::Var(id) => self.get_value(*id),

            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let lhs_value = self.translate_value(lhs)?.into_int_value();
                let rhs_value = self.translate_value(rhs)?.into_int_value();

                match operation {
                    BinaryOperation::Lt | BinaryOperation::Gt | BinaryOperation::Eq => {
                        let (lhs_cmp, rhs_cmp) = self
                            .try_narrow_comparison(context, lhs_value, rhs_value, lhs.id, rhs.id)?;
                        self.generate_binop(*operation, lhs_cmp, rhs_cmp, context)
                    }
                    BinaryOperation::Slt | BinaryOperation::Sgt => {
                        self.generate_signed_comparison(*operation, lhs_value, rhs_value, context)
                    }

                    BinaryOperation::And | BinaryOperation::Or | BinaryOperation::Xor => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_value =
                                    self.ensure_exact_width(context, lhs_value, 64, "dnbit_l")?;
                                let rhs_value =
                                    self.ensure_exact_width(context, rhs_value, 64, "dnbit_r")?;
                                return self
                                    .generate_binop(*operation, lhs_value, rhs_value, context);
                            }
                            if db <= 128 {
                                let lhs_value =
                                    self.ensure_exact_width(context, lhs_value, 128, "dnbit128_l")?;
                                let rhs_value =
                                    self.ensure_exact_width(context, rhs_value, 128, "dnbit128_r")?;
                                return self
                                    .generate_binop(*operation, lhs_value, rhs_value, context);
                            }
                        }
                        let (lhs_value, rhs_value) =
                            self.ensure_same_type(context, lhs_value, rhs_value, "bitwise")?;
                        self.generate_binop(*operation, lhs_value, rhs_value, context)
                    }

                    BinaryOperation::Add | BinaryOperation::Sub | BinaryOperation::Mul => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let lhs_value =
                                    self.ensure_exact_width(context, lhs_value, 64, "dnarith_l")?;
                                let rhs_value =
                                    self.ensure_exact_width(context, rhs_value, 64, "dnarith_r")?;
                                return self
                                    .generate_binop(*operation, lhs_value, rhs_value, context);
                            }
                            if db <= 128 {
                                let lhs_value = self.ensure_exact_width(
                                    context,
                                    lhs_value,
                                    128,
                                    "dnarith128_l",
                                )?;
                                let rhs_value = self.ensure_exact_width(
                                    context,
                                    rhs_value,
                                    128,
                                    "dnarith128_r",
                                )?;
                                return self
                                    .generate_binop(*operation, lhs_value, rhs_value, context);
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
                            let (lhs_value, rhs_value) =
                                self.ensure_min_width(context, lhs_value, rhs_value, 64, "arith")?;
                            self.generate_binop(*operation, lhs_value, rhs_value, context)
                        } else if result_fits_i128 {
                            let (lhs_value, rhs_value) = self
                                .ensure_min_width(context, lhs_value, rhs_value, 128, "arith128")?;
                            self.generate_binop(*operation, lhs_value, rhs_value, context)
                        } else {
                            let lhs_value =
                                self.ensure_word_type(context, lhs_value, "arith_lhs")?;
                            let rhs_value =
                                self.ensure_word_type(context, rhs_value, "arith_rhs")?;
                            self.generate_binop(*operation, lhs_value, rhs_value, context)
                        }
                    }

                    BinaryOperation::Div | BinaryOperation::Mod => {
                        let lhs_width = lhs_value.get_type().get_bit_width();
                        let rhs_width = rhs_value.get_type().get_bit_width();
                        if lhs_width <= 64 && rhs_width <= 64 {
                            let (lhs_value, rhs_value) = self.ensure_same_type(
                                context,
                                lhs_value,
                                rhs_value,
                                "narrow_divmod",
                            )?;
                            self.generate_narrow_divmod(*operation, lhs_value, rhs_value, context)
                        } else {
                            let lhs_value =
                                self.ensure_word_type(context, lhs_value, "binop_lhs")?;
                            let rhs_value =
                                self.ensure_word_type(context, rhs_value, "binop_rhs")?;
                            self.generate_binop(*operation, lhs_value, rhs_value, context)
                        }
                    }

                    BinaryOperation::Shl => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                if let Some(shift) = Self::try_get_small_constant(lhs_value) {
                                    if shift >= 64 {
                                        let i64_type = context.llvm().i64_type();
                                        return Ok(i64_type.const_zero().as_basic_value_enum());
                                    }
                                    let rhs_narrow = self.ensure_exact_width(
                                        context,
                                        rhs_value,
                                        64,
                                        "dnshl_val",
                                    )?;
                                    let lhs_narrow = self.ensure_exact_width(
                                        context,
                                        lhs_value,
                                        64,
                                        "dnshl_amt",
                                    )?;
                                    let result = context
                                        .builder()
                                        .build_left_shift(rhs_narrow, lhs_narrow, "shl_dn")
                                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                                    return Ok(result.as_basic_value_enum());
                                }
                            }
                        }
                        let lhs_value = self.ensure_word_type(context, lhs_value, "binop_lhs")?;
                        let rhs_value = self.ensure_word_type(context, rhs_value, "binop_rhs")?;
                        self.generate_binop(*operation, lhs_value, rhs_value, context)
                    }

                    BinaryOperation::Shr => {
                        let rhs_inferred = self.inferred_width(rhs.id);
                        if rhs_inferred.bits() <= 64 {
                            if let Some(shift) = Self::try_get_small_constant(lhs_value) {
                                if shift >= 64 {
                                    let i64_type = context.llvm().i64_type();
                                    return Ok(i64_type.const_zero().as_basic_value_enum());
                                }
                                let rhs_narrow =
                                    self.ensure_exact_width(context, rhs_value, 64, "dnshr_val")?;
                                let lhs_narrow =
                                    self.ensure_exact_width(context, lhs_value, 64, "dnshr_amt")?;
                                let result = context
                                    .builder()
                                    .build_right_shift(rhs_narrow, lhs_narrow, false, "shr_dn")
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                                return Ok(result.as_basic_value_enum());
                            }
                        }
                        let lhs_value = self.ensure_word_type(context, lhs_value, "binop_lhs")?;
                        let rhs_value = self.ensure_word_type(context, rhs_value, "binop_rhs")?;
                        self.generate_binop(*operation, lhs_value, rhs_value, context)
                    }

                    _ => {
                        let lhs_value = self.ensure_word_type(context, lhs_value, "binop_lhs")?;
                        let rhs_value = self.ensure_word_type(context, rhs_value, "binop_rhs")?;
                        self.generate_binop(*operation, lhs_value, rhs_value, context)
                    }
                }
            }

            Expression::Ternary {
                operation,
                a: first_operand,
                b: second_operand,
                n: modulus,
            } => {
                let a_value = self.translate_value(first_operand)?.into_int_value();
                let b_value = self.translate_value(second_operand)?.into_int_value();
                let n_value = self.translate_value(modulus)?.into_int_value();
                let a_value = self.ensure_word_type(context, a_value, "ternary_a")?;
                let b_value = self.ensure_word_type(context, b_value, "ternary_b")?;
                let n_value = self.ensure_word_type(context, n_value, "ternary_n")?;

                match operation {
                    BinaryOperation::AddMod => Ok(revive_llvm_context::polkavm_evm_math::add_mod(
                        context, a_value, b_value, n_value,
                    )?),
                    BinaryOperation::MulMod => Ok(revive_llvm_context::polkavm_evm_math::mul_mod(
                        context, a_value, b_value, n_value,
                    )?),
                    _ => Err(CodegenError::Unsupported(format!(
                        "Ternary operation {:?}",
                        operation
                    ))),
                }
            }

            Expression::Unary { operation, operand } => {
                let operand_value = self.translate_value(operand)?.into_int_value();
                match operation {
                    UnaryOperation::IsZero => {
                        let zero = operand_value.get_type().const_zero();
                        let is_zero = context
                            .builder()
                            .build_int_compare(
                                inkwell::IntPredicate::EQ,
                                operand_value,
                                zero,
                                "iszero",
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        Ok(is_zero.as_basic_value_enum())
                    }
                    UnaryOperation::Not => {
                        if let Some(db) = demand_bits {
                            if db <= 64 {
                                let narrow_value = self.ensure_exact_width(
                                    context,
                                    operand_value,
                                    64,
                                    "dnnot_op",
                                )?;
                                let all_ones = narrow_value.get_type().const_all_ones();
                                let xor_result = context
                                    .builder()
                                    .build_xor(narrow_value, all_ones, "not_narrow")
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                                return Ok(xor_result.as_basic_value_enum());
                            }
                        }
                        let operand_value =
                            self.ensure_word_type(context, operand_value, "not_op")?;
                        let all_ones = context.word_type().const_all_ones();
                        let xor_result = context
                            .builder()
                            .build_xor(operand_value, all_ones, "not_result")
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                        Ok(xor_result.as_basic_value_enum())
                    }
                    UnaryOperation::Clz => {
                        let operand_value =
                            self.ensure_word_type(context, operand_value, "clz_op")?;
                        Ok(
                            revive_llvm_context::polkavm_evm_bitwise::count_leading_zeros(
                                context,
                                operand_value,
                            )?,
                        )
                    }
                }
            }

            Expression::CallDataLoad { offset } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                if self.use_outlined_calldataload {
                    Ok(revive_llvm_context::polkavm_evm_calldata::load_outlined(
                        context,
                        offset_value,
                    )?)
                } else {
                    Ok(revive_llvm_context::polkavm_evm_calldata::load(
                        context,
                        offset_value,
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
                let address_value = self.translate_value(address)?.into_int_value();
                let address_value =
                    self.ensure_word_type(context, address_value, "extcodesize_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::size(
                    context,
                    Some(address_value),
                )?)
            }

            Expression::ReturnDataSize => {
                Ok(revive_llvm_context::polkavm_evm_return_data::size(context)?)
            }

            Expression::ExtCodeHash { address } => {
                let address_value = self.translate_value(address)?.into_int_value();
                let address_value =
                    self.ensure_word_type(context, address_value, "extcodehash_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ext_code::hash(
                    context,
                    address_value,
                )?)
            }

            Expression::BlockHash { number } => {
                let num_value = self.translate_value(number)?.into_int_value();
                let num_value = self.ensure_word_type(context, num_value, "blockhash_num")?;
                Ok(
                    revive_llvm_context::polkavm_evm_contract_context::block_hash(
                        context, num_value,
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
                let address_value = self.translate_value(address)?.into_int_value();
                let address_value =
                    self.ensure_word_type(context, address_value, "balance_addr")?;
                Ok(revive_llvm_context::polkavm_evm_ether_gas::balance(
                    context,
                    address_value,
                )?)
            }

            Expression::MLoad { offset, region } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                self.advance_msize_watermark(context, offset_value)?;
                let is_free_pointer = matches!(region, MemoryRegion::FreePointerSlot)
                    || Self::is_free_pointer_load(offset_value);

                let loaded = match self.native_memory_mode(context, offset_value) {
                    NativeMemoryMode::AllNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mload_offset_xlen",
                        )?;
                        context.build_load_native(offset_xlen)?
                    }
                    NativeMemoryMode::InlineNative => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mload_offset_xlen",
                        )?;
                        let pointer = context.build_heap_gep_unchecked(offset_xlen)?;
                        if is_free_pointer && !self.heap_opt.fmp_could_be_unbounded() {
                            let narrow = context
                                .builder()
                                .build_load(context.xlen_type(), pointer.value, "fmp_load")
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                .into_int_value();
                            context
                                .builder()
                                .build_int_z_extend(narrow, context.word_type(), "fmp_zext")
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                .as_basic_value_enum()
                        } else {
                            context
                                .builder()
                                .build_load(context.word_type(), pointer.value, "native_load")
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                .as_basic_value_enum()
                        }
                    }
                    NativeMemoryMode::InlineByteSwap => {
                        let offset_xlen = self.truncate_offset_to_xlen(
                            context,
                            offset_value,
                            "mload_offset_xlen",
                        )?;
                        revive_llvm_context::polkavm_evm_memory::load_bswap_unchecked(
                            context,
                            offset_xlen,
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                    }
                    NativeMemoryMode::ByteSwap => {
                        let offset_value = self.narrow_offset_for_pointer(
                            context,
                            offset_value,
                            offset.id,
                            "mload_offset_narrow",
                        )?;
                        if !self.has_msize {
                            let offset_xlen = context
                                .safe_truncate_int_to_xlen(offset_value)
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                            let function = self.get_or_create_load_bswap_checked_fn(context)?;
                            context
                                .builder()
                                .build_call(function, &[offset_xlen.into()], "checked_load")
                                .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                .try_as_basic_value()
                                .basic()
                                .expect("load_bswap_checked should return a value")
                        } else {
                            revive_llvm_context::polkavm_evm_memory::load(context, offset_value)?
                        }
                    }
                };
                if is_free_pointer && !self.heap_opt.fmp_could_be_unbounded() {
                    Self::apply_free_pointer_range_proof(context, loaded)
                } else {
                    Ok(loaded)
                }
            }

            Expression::SLoad {
                key,
                static_slot: _,
            } => {
                let key_argument = self.value_to_storage_key_argument(key, context)?;
                if key_argument.is_register() {
                    let key_value = key_argument
                        .access(context)
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    let sload_fn = self.get_or_create_sload_word_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(sload_fn, &[key_value.into()], "sload_word")
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                        .try_as_basic_value()
                        .basic()
                        .expect("sload_word should return a value");
                    Ok(result)
                } else {
                    Ok(revive_llvm_context::polkavm_evm_storage::load(
                        context,
                        &key_argument,
                    )?)
                }
            }

            Expression::TLoad { key } => {
                let key_argument = self.value_to_argument(key, context)?;
                Ok(revive_llvm_context::polkavm_evm_storage::transient_load(
                    context,
                    &key_argument,
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

                let parameter_types = self.function_parameter_types.get(&function.0).cloned();
                let mut argument_values = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let parameter_type = parameter_types
                        .as_ref()
                        .and_then(|parameter_types| parameter_types.get(index));
                    let value = match parameter_type {
                        Some(Type::Int(width)) if *width < BitWidth::I256 => {
                            let llvm_value = self.translate_value(argument)?;
                            let integer_value = llvm_value.into_int_value();
                            let target_type = context.integer_type(width.bits() as usize);
                            let argument_bits = integer_value.get_type().get_bit_width();
                            let target_bits = target_type.get_bit_width();
                            if argument_bits > target_bits {
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
                                            &format!("call_arg_narrow_{}", index),
                                        )
                                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                        .as_basic_value_enum()
                                } else {
                                    self.checked_truncate_to(
                                        context,
                                        integer_value,
                                        target_type,
                                        &format!("call_arg_narrow_{}", index),
                                    )?
                                    .as_basic_value_enum()
                                }
                            } else if argument_bits < target_bits {
                                context
                                    .builder()
                                    .build_int_z_extend(
                                        integer_value,
                                        target_type,
                                        &format!("call_arg_widen_{}", index),
                                    )
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
                                    .as_basic_value_enum()
                            } else {
                                integer_value.as_basic_value_enum()
                            }
                        }
                        _ => self.translate_value_as_word(
                            argument,
                            context,
                            &format!("call_arg_{}", index),
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
                    if let Some(first_argument) = argument_values.first() {
                        self.emit_validator_assume(context, *first_argument, mask)?;
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
                                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
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
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::ZeroExtend { value, to } => {
                let value = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_z_extend(value, target_type, "zext")
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::SignExtendTo { value, to } => {
                let value = self.translate_value(value)?.into_int_value();
                let target_type = context.integer_type(to.bits() as usize);
                Ok(context
                    .builder()
                    .build_int_s_extend(value, target_type, "sext")
                    .map_err(|error| CodegenError::Llvm(error.to_string()))?
                    .as_basic_value_enum())
            }

            Expression::Keccak256 { offset, length } => {
                let offset_value = self.translate_value(offset)?.into_int_value();
                let offset_value = self.narrow_offset_for_pointer(
                    context,
                    offset_value,
                    offset.id,
                    "keccak_offset_narrow",
                )?;
                let length_value = self.translate_value(length)?.into_int_value();
                let length_value = self.narrow_offset_for_pointer(
                    context,
                    length_value,
                    length.id,
                    "keccak_length_narrow",
                )?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3(
                    context,
                    offset_value,
                    length_value,
                )?)
            }

            Expression::Keccak256Pair { word0, word1 } => {
                let word0_value = self.translate_value(word0)?.into_int_value();
                let word0_value = self.ensure_word_type(context, word0_value, "keccak_word0")?;
                let word1_value = self.translate_value(word1)?.into_int_value();
                let word1_value = self.ensure_word_type(context, word1_value, "keccak_word1")?;

                if word1_value.is_const() && Self::try_extract_const_u64(word1_value).is_none() {
                    let wrapper_fn =
                        self.get_or_create_keccak256_slot_wrapper(word1_value, context)?;
                    let function_type = context
                        .word_type()
                        .fn_type(&[context.word_type().into()], false);
                    let result = context
                        .builder()
                        .build_indirect_call(
                            function_type,
                            wrapper_fn.as_global_value().as_pointer_value(),
                            &[word0_value.into()],
                            "keccak256_slot_call",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
                    Ok(result
                        .try_as_basic_value()
                        .basic()
                        .expect("keccak256 slot wrapper should return a value"))
                } else {
                    Ok(revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                        context,
                        word0_value,
                        word1_value,
                    )?)
                }
            }

            Expression::Keccak256Single { word0 } => {
                let word0_value = self.translate_value(word0)?.into_int_value();
                let word0_value = self.ensure_word_type(context, word0_value, "keccak_word0")?;
                Ok(revive_llvm_context::polkavm_evm_crypto::sha3_one_word(
                    context,
                    word0_value,
                )?)
            }

            Expression::DataOffset { id } => {
                let argument =
                    revive_llvm_context::polkavm_evm_create::contract_hash(context, id.clone())?;
                argument
                    .access(context)
                    .map_err(|error| CodegenError::Llvm(error.to_string()))
            }

            Expression::DataSize { id } => {
                let argument =
                    revive_llvm_context::polkavm_evm_create::header_size(context, id.clone())?;
                argument
                    .access(context)
                    .map_err(|error| CodegenError::Llvm(error.to_string()))
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
                let key_value = self.translate_value(key)?.into_int_value();
                let key_value = self.ensure_word_type(context, key_value, "mapping_sload_key")?;
                let slot_value = self.translate_value(slot)?.into_int_value();
                let slot_value =
                    self.ensure_word_type(context, slot_value, "mapping_sload_slot")?;

                if self.use_outlined_mapping_sload {
                    let mapping_sload_fn = self.get_or_create_mapping_sload_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(
                            mapping_sload_fn,
                            &[key_value.into(), slot_value.into()],
                            "mapping_sload_call",
                        )
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
                        .try_as_basic_value()
                        .basic()
                        .expect("mapping_sload should return a value");
                    Ok(result)
                } else {
                    let hash_value = if slot_value.is_const()
                        && Self::try_extract_const_u64(slot_value).is_none()
                    {
                        let wrapper_fn =
                            self.get_or_create_keccak256_slot_wrapper(slot_value, context)?;
                        let function_type = context
                            .word_type()
                            .fn_type(&[context.word_type().into()], false);
                        context
                            .builder()
                            .build_indirect_call(
                                function_type,
                                wrapper_fn.as_global_value().as_pointer_value(),
                                &[key_value.into()],
                                "keccak256_slot_call",
                            )
                            .map_err(|error| CodegenError::Llvm(error.to_string()))?
                            .try_as_basic_value()
                            .basic()
                            .expect("keccak256 slot wrapper should return a value")
                            .into_int_value()
                    } else {
                        revive_llvm_context::polkavm_evm_crypto::sha3_two_words(
                            context, key_value, slot_value,
                        )?
                        .into_int_value()
                    };
                    let sload_fn = self.get_or_create_sload_word_fn(context)?;
                    let result = context
                        .builder()
                        .build_call(sload_fn, &[hash_value.into()], "mapping_sload_word")
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?
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
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;

        let non_zero_block = context.append_basic_block("divmod_nonzero");
        let join_block = context.append_basic_block("divmod_join");
        let current_block = context.basic_block();

        context.build_conditional_branch(is_zero, join_block, non_zero_block)?;

        context.set_basic_block(non_zero_block);
        let result = match operation {
            BinaryOperation::Div => context
                .builder()
                .build_int_unsigned_div(lhs, rhs, "narrow_div")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?,
            BinaryOperation::Mod => context
                .builder()
                .build_int_unsigned_rem(lhs, rhs, "narrow_mod")
                .map_err(|error| CodegenError::Llvm(error.to_string()))?,
            _ => unreachable!(),
        };
        let non_zero_exit = context.basic_block();
        context.build_unconditional_branch(join_block);

        context.set_basic_block(join_block);
        let phi = context
            .builder()
            .build_phi(int_type, "divmod_result")
            .map_err(|error| CodegenError::Llvm(error.to_string()))?;
        phi.add_incoming(&[(&zero, current_block), (&result, non_zero_exit)]);

        Ok(phi.as_basic_value().as_basic_value_enum())
    }

    /// Generates a signed comparison (`slt`/`sgt`) at full word width.
    ///
    /// Signed comparisons must run at full width. A narrowed operand is
    /// provably non-negative (newyork never narrows signed values), so a set
    /// top bit at the narrow width is not a sign bit — comparing at that width
    /// misreads it as negative (e.g. 1 in i1 is -1, 0xC8 in i8 is -56), which
    /// diverges from EVM's 256-bit signed comparison. Both operands are
    /// zero-extended to the full word before comparing.
    fn generate_signed_comparison(
        &mut self,
        operation: BinaryOperation,
        lhs_value: IntValue<'ctx>,
        rhs_value: IntValue<'ctx>,
        context: &mut PolkaVMContext<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let lhs_value = self.ensure_word_type(context, lhs_value, "scmp_lhs")?;
        let rhs_value = self.ensure_word_type(context, rhs_value, "scmp_rhs")?;
        self.generate_binop(operation, lhs_value, rhs_value, context)
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
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
                        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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
        let llvm_value = self.translate_value(value)?;
        if llvm_value.is_int_value() {
            let integer_value = llvm_value.into_int_value();
            Ok(self
                .ensure_word_type(context, integer_value, name)?
                .as_basic_value_enum())
        } else {
            Ok(llvm_value)
        }
    }

    /// Converts a Value to a PolkaVMArgument for storage operations.
    /// Storage operations require 256-bit values, so narrow values are zero-extended.
    fn value_to_argument(
        &self,
        value: &Value,
        context: &PolkaVMContext<'ctx>,
    ) -> Result<PolkaVMArgument<'ctx>> {
        let llvm_value = self.translate_value(value)?;
        if llvm_value.is_int_value() {
            let integer_value = llvm_value.into_int_value();
            let word_value = self.ensure_word_type(context, integer_value, "storage_arg")?;
            Ok(PolkaVMArgument::value(word_value.as_basic_value_enum()))
        } else {
            Ok(PolkaVMArgument::value(llvm_value))
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

        let noinline_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::NoInline as u32, 0);
        let optsize_attribute = context.llvm().create_enum_attribute(
            revive_llvm_context::PolkaVMAttribute::OptimizeForSize as u32,
            0,
        );
        let minsize_attribute = context
            .llvm()
            .create_enum_attribute(revive_llvm_context::PolkaVMAttribute::MinSize as u32, 0);
        wrapper_fn.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            noinline_attribute,
        );
        wrapper_fn.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            optsize_attribute,
        );
        wrapper_fn.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            minsize_attribute,
        );

        let saved_block = context.basic_block();

        let entry_block = context.llvm().append_basic_block(wrapper_fn, "entry");
        context.set_basic_block(entry_block);

        let word0_parameter = wrapper_fn.get_nth_param(0).unwrap().into_int_value();

        let keccak_fn = context
            .get_function(
                revive_llvm_context::PolkaVMKeccak256TwoWordsFunction::NAME,
                false,
            )
            .expect("__revive_keccak256_two_words should be declared");

        let result = context
            .build_call(
                keccak_fn.borrow().declaration(),
                &[word0_parameter.into(), slot_const.into()],
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
        let llvm_value = self.translate_value(value)?;
        if !llvm_value.is_int_value() {
            return Ok(PolkaVMArgument::value(llvm_value));
        }

        let integer_value = llvm_value.into_int_value();
        let word_value = self.ensure_word_type(context, integer_value, "storage_key")?;

        if !word_value.is_const() {
            return Ok(PolkaVMArgument::value(word_value.as_basic_value_enum()));
        }

        if Self::try_extract_const_u64(word_value).is_some() {
            return Ok(PolkaVMArgument::value(word_value.as_basic_value_enum()));
        }

        let const_str = word_value.print_to_string().to_string();

        if let Some(&global_pointer) = self.storage_key_globals.get(&const_str) {
            let pointer = revive_llvm_context::PolkaVMPointer::new(
                context.word_type(),
                Default::default(),
                global_pointer,
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
            global.set_initializer(&word_value);
            global.set_alignment(32);

            let global_pointer = global.as_pointer_value();
            self.storage_key_globals.insert(const_str, global_pointer);

            let pointer = revive_llvm_context::PolkaVMPointer::new(
                context.word_type(),
                Default::default(),
                global_pointer,
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
        .map_err(|error| CodegenError::Llvm(error.to_string()))?;
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

#[cfg(test)]
mod then_region_aborts_tests {
    use super::then_region_aborts;
    use crate::ir::{Expression, FunctionId, Region, Statement, Value, ValueId};
    use std::collections::BTreeSet;

    fn region(statements: Vec<Statement>) -> Region {
        Region {
            statements,
            yields: vec![],
        }
    }

    fn call(function: u32) -> Statement {
        Statement::Expression(Expression::Call {
            function: FunctionId(function),
            arguments: vec![],
        })
    }

    fn revert() -> Statement {
        Statement::Revert {
            offset: Value::int(ValueId(0)),
            length: Value::int(ValueId(0)),
        }
    }

    #[test]
    fn explicit_terminator_aborts() {
        let noreturn = BTreeSet::new();
        assert!(then_region_aborts(&region(vec![revert()]), &noreturn));
        assert!(then_region_aborts(
            &region(vec![Statement::Invalid]),
            &noreturn
        ));
    }

    /// A call to a function NOT proven noreturn does not abort: a failure
    /// handler that returns leaves the validator argument unconstrained, so no
    /// assume may be emitted. The same call to a noreturn function does abort.
    #[test]
    fn trailing_call_aborts_only_when_noreturn() {
        let none: BTreeSet<u32> = BTreeSet::new();
        assert!(!then_region_aborts(&region(vec![call(7)]), &none));

        let noreturn: BTreeSet<u32> = [7u32].into_iter().collect();
        assert!(then_region_aborts(&region(vec![call(7)]), &noreturn));
    }

    #[test]
    fn returning_call_after_lets_does_not_abort() {
        let none: BTreeSet<u32> = BTreeSet::new();
        let statements = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::CallValue,
            },
            call(7),
        ];
        assert!(!then_region_aborts(&region(statements), &none));
    }
}
