//! Heap optimization pass for partial heap emulation.
//!
//! This module implements an optimization strategy for handling big-endian EVM
//! memory operations on the little-endian PolkaVM/RISC-V target. The approach:
//!
//! 1. Start with a fully little-endian heap
//! 2. At compile time, analyze memory access patterns to determine alignment
//! 3. Mark regions that require big-endian emulation ("tainted" regions)
//! 4. Generate optimized code that avoids byte-swapping when possible
//!
//! # Memory Access Analysis
//!
//! Memory accesses are classified into:
//! - **Aligned accesses**: Offset is known at compile time and word-aligned (multiple of 32)
//! - **Potentially unaligned**: Offset is computed dynamically or not word-aligned
//!
//! For aligned accesses, we can often eliminate byte-swapping by keeping values
//! in native little-endian format when they don't escape to external calls.

use std::collections::{BTreeMap, BTreeSet};

use num::BigUint;

use crate::ir::{
    for_each_statement, word_align, Block, Expression, FunctionId, MemoryRegion, Object, Statement,
    Value, ValueId,
};
use revive_common::BYTE_LENGTH_WORD;

/// Maximum number of words to iterate when marking escaping/tainted ranges.
/// Contracts with `return(0, 320000000000)` or similar huge constants would
/// cause billions of loop iterations without this cap. Any range exceeding
/// this is treated as a dynamic escape instead.
const MAX_RANGE_WORDS: u64 = 4096;

/// Classification of a memory access pattern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccessPattern {
    /// Offset is known at compile time and word-aligned (multiple of 32).
    AlignedStatic(u64),
    /// Offset is known at compile time but not word-aligned.
    UnalignedStatic(u64),
    /// Offset is computed dynamically but provably aligned.
    AlignedDynamic,
    /// Offset is computed dynamically and may be unaligned.
    Unknown,
}

impl AccessPattern {
    /// Returns true if this access is known to be aligned.
    pub fn is_aligned(&self) -> bool {
        matches!(
            self,
            AccessPattern::AlignedStatic(_) | AccessPattern::AlignedDynamic
        )
    }

    /// Returns true if this access pattern is fully known at compile time.
    pub fn is_static(&self) -> bool {
        matches!(
            self,
            AccessPattern::AlignedStatic(_) | AccessPattern::UnalignedStatic(_)
        )
    }
}

/// Memory slot tracking for heap analysis.
#[derive(Clone, Debug)]
pub struct MemorySlot {
    /// The memory region this slot belongs to.
    pub region: MemoryRegion,
    /// Whether this slot has been written with potentially unaligned data.
    pub tainted: bool,
    /// Whether this slot escapes (used in external calls, returns, etc.).
    pub escapes: bool,
}

impl Default for MemorySlot {
    fn default() -> Self {
        MemorySlot {
            region: MemoryRegion::Unknown,
            tainted: false,
            escapes: false,
        }
    }
}

/// Heap optimization analysis context.
pub struct HeapAnalysis {
    /// The configured EVM heap size in bytes.
    heap_size: u64,
    /// Known static memory offsets and their access patterns.
    memory_accesses: BTreeMap<u64, AccessPattern>,
    /// Values known to be memory offsets (for tracking alignment).
    offset_values: BTreeMap<u32, OffsetInfo>,
    /// Memory regions that are known to be tainted (require big-endian emulation).
    tainted_regions: BTreeSet<u64>,
    /// Memory regions that escape to external code.
    escaping_regions: BTreeSet<u64>,
    /// Whether any memory escaping statement (return, revert, external call, log, create)
    /// has a dynamic (non-static) offset. When true, we cannot determine which regions
    /// escape and must conservatively disable native-only mode.
    has_dynamic_escapes: bool,
    /// The minimum static start offset of any dynamic-length escape.
    /// When a return/revert/call has a known start but unknown length, all memory
    /// from this offset onwards could potentially escape.
    /// `None` means no such escape exists, or the start offset is also dynamic.
    min_dynamic_escape_start: Option<u64>,
    /// Whether any memory access (mstore, mstore8, mcopy, mload) has a dynamic
    /// (non-static) offset that we cannot track. When true, some accesses are invisible
    /// to the analysis.
    has_dynamic_accesses: bool,
    /// Whether any statement sends memory to external code over a static range
    /// that covers the FMP word at 0x40 (`return`, `revert`, `log*`, external
    /// calls, `create*`, `keccak256`). When true, user data stored at 0x40
    /// would be observed by the caller in BE format, so the FMP native-mode
    /// optimization is unsafe. Normal Solidity returns and reverts from
    /// `free_ptr (>= 0x80)` so this fires only for inline-assembly patterns
    /// like `return(0, 96)` / `revert(0, 96)`.
    fmp_word_escapes: bool,
    /// Static offsets that are accessed via non-literal (variable) expressions.
    /// When the solc M3 optimizer turns literal offsets into variables
    /// (e.g., `let size := 64; mload(size)`), the LLVM IR value won't be a constant.
    /// Native mode requires LLVM constant detection, so these offsets must use
    /// byte-swap mode to avoid store/load mode mismatches.
    variable_accessed_offsets: BTreeSet<u64>,
    /// Whether the FMP slot at 0x40 is written from a value that is not
    /// provably sbrk-bounded. Solidity's allocator only ever writes
    /// either a literal initial value (`0x80` from `memoryguard`) or
    /// `add(mload(0x40), bounded_size)` — both keep the FMP < heap_size
    /// in practice. Inline asm such as `mstore(0x40, calldataload(0))`
    /// can put any 256-bit value at 0x40. When true, downstream
    /// optimizations that assume `FMP < heap_size` (e.g. the post-MLoad
    /// range proof) must skip the optimization to preserve EVM
    /// semantics. Conservatively starts false; set by inspecting every
    /// `MStore` whose target is the FMP slot.
    ///
    /// **Known gap (deliberate).** A dynamic-offset full-word `MStore` sets this flag when its
    /// offset is computed from literals and literal-seeded loop counters and can drop below `0x60`
    /// ([`OffsetInfo::iteration_range`]), or when its region is `Scratch`, which bounds only its
    /// first byte. Any other dynamic offset does not set it, even though it
    /// could land on the FMP word `[0x40, 0x5f]` directly or by wrapping (mod 2^256) and overwrite
    /// the pointer with an arbitrary value. There is no cheap sound discriminator: the wrapped
    /// offset is in-bounds (`safe_truncate` only traps offsets `>= heap_size`), and 256-bit wrap
    /// lets any computed offset reach `0x40`, so it cannot be proven to miss the slot from
    /// width/range info. Flagging every dynamic full-word store — as the dynamic `mstore8` branch
    /// does, where it is cheap because byte writes are rare — would disable the FMP optimization
    /// for essentially every contract (~+9% / +30 KB on the OZ corpus). The gap is
    /// solc-unreachable: solc's dynamic stores are FMP-relative (`>= 0x80`) and never target
    /// `0x40`; only hand-written Yul with an offset engineered to equal `0x40` reaches it.
    ///
    /// A *static* misaligned full-word store that overlaps the FMP word
    /// ([`crate::ir::word_store_overlaps_free_pointer_slot`], e.g. `mstore(0x21, v)`) corrupts the
    /// pointer with arbitrary bytes. solc emits exactly such stores when ABI-encoding a revert
    /// string (`mstore(0x24, len)` / `mstore(0x44, data)` cover `0x40`), so flagging every one
    /// unconditionally would set this for almost every contract with a `require`-with-message
    /// (~+12 % / +13 KB on the OZ corpus). Those revert-encoding stores are benign — they precede
    /// an immediate frame terminator, so the corrupted pointer is never read back. For this class of
    /// store, the flag is therefore set only when the corruption is *observed*:
    /// [`Self::detect_observed_fmp_corruption`] walks control flow (see [`Self::scan_fmp_corruption`]
    /// and [`Self::fmp_corrupting_functions`]) and sets it when an overlap store's corruption reaches
    /// a later `mload(0x40)`, or a call / error-string encoding that reads it. The FMP value-forwarding consumer is additionally defended at
    /// the corruption site (see the `word_store_overlaps_free_pointer_slot` branch in `mem_opt`'s
    /// `FmpPropagation`).
    fmp_could_be_unbounded: bool,
    /// Per-`Let`-binding source expression, used by
    /// `is_trusted_fmp_source` to decide whether an mstore's value
    /// comes from a sbrk-style allocator pattern. Only populated for
    /// bindings of size 1.
    value_expressions: BTreeMap<u32, Expression>,
    /// Functions of the currently analyzed object that can exit — by `leave` or by body
    /// fall-through — with the free-memory-pointer word corrupted by an overlapping store.
    /// Computed to a fixed point by [`Self::detect_observed_fmp_corruption`] and consulted at
    /// call sites by [`Self::scan_fmp_corruption`], so corruption escaping a callee is visible
    /// to the caller's observation scan.
    fmp_corrupting_functions: BTreeSet<FunctionId>,
    /// Dynamic destinations of writes that may cover the FMP word unless they are free pointer
    /// relative. They are checked once the whole object is analyzed, because a destination
    /// derived from a function parameter or a call result depends on every call site.
    deferred_write_destinations: Vec<u32>,
    /// Parameters for which every call site passes a free pointer relative argument.
    free_pointer_relative_parameters: BTreeSet<u32>,
    /// Single-return functions whose every return value is free pointer relative.
    free_pointer_relative_returns: BTreeSet<FunctionId>,
    /// Branch, switch and loop outputs whose every incoming value is free pointer relative.
    free_pointer_relative_merges: BTreeSet<u32>,
}

/// Information about a value used as a memory offset.
#[derive(Clone, Debug)]
pub struct OffsetInfo {
    /// The value itself, when the analysis knows it is the same constant on every evaluation.
    pub static_value: Option<u64>,
    /// Known alignment (in bytes). 32 means word-aligned.
    pub alignment: u32,
    /// Whether the IR value is itself a literal expression rather than a variable, which decides
    /// whether the LLVM IR value is a constant. When false, the LLVM IR value may not be a
    /// constant even though `static_value` resolves it.
    pub from_literal: bool,
    /// The range of values it takes over all its evaluations, including every loop iteration, when
    /// it is computed only from literals and literal-seeded loop counters, whether or not it has a
    /// `static_value`. `None` when it has any other source, such as `mload(0x40)`, a parameter or a call.
    pub iteration_range: Option<IterationRange>,
}

impl Default for OffsetInfo {
    fn default() -> Self {
        OffsetInfo {
            static_value: None,
            alignment: 1,
            from_literal: false,
            iteration_range: None,
        }
    }
}

/// The range of values that an offset computed only from literals and literal-seeded loop counters
/// takes over all its evaluations ([`OffsetInfo::iteration_range`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationRange {
    /// Never below `minimum` and always below `2^bit_ceiling`, where `bit_ceiling` is at most
    /// [`revive_common::BIT_LENGTH_WORD`]: the exact result of every operation that computed the
    /// value fits the word, so none wrapped around 2^256 and `minimum` holds.
    AtLeast {
        /// A lower bound of the value on every evaluation.
        minimum: u64,
        /// An upper bound of the value's bit length on every evaluation.
        bit_ceiling: u32,
    },
    /// No provable minimum: the value may descend or wrap around 2^256, or went through an
    /// operation other than `add`, `mul` and `shl` by a static shift.
    Unknown,
}

impl IterationRange {
    /// The range of a value that is always `value`.
    fn exactly(value: u64) -> Self {
        Self::AtLeast {
            minimum: value,
            bit_ceiling: u64::BITS - value.leading_zeros(),
        }
    }

    /// `AtLeast`, unless `bit_ceiling` exceeds the word: the value may then have wrapped around
    /// 2^256, and `minimum` no longer holds.
    fn at_least(minimum: u64, bit_ceiling: u32) -> Self {
        if bit_ceiling as usize <= revive_common::BIT_LENGTH_WORD {
            Self::AtLeast {
                minimum,
                bit_ceiling,
            }
        } else {
            Self::Unknown
        }
    }

    /// The range of `add(lhs, rhs)` for `lhs` in `self` and `rhs` in `other`.
    fn plus(self, other: Self) -> Self {
        match (self, other) {
            (
                Self::AtLeast {
                    minimum: lhs_minimum,
                    bit_ceiling: lhs_ceiling,
                },
                Self::AtLeast {
                    minimum: rhs_minimum,
                    bit_ceiling: rhs_ceiling,
                },
            ) => Self::at_least(
                lhs_minimum.saturating_add(rhs_minimum),
                lhs_ceiling.max(rhs_ceiling) + 1,
            ),
            _ => Self::Unknown,
        }
    }

    /// The range of `mul(lhs, rhs)` for `lhs` in `self` and `rhs` in `other`.
    fn times(self, other: Self) -> Self {
        match (self, other) {
            (
                Self::AtLeast {
                    minimum: lhs_minimum,
                    bit_ceiling: lhs_ceiling,
                },
                Self::AtLeast {
                    minimum: rhs_minimum,
                    bit_ceiling: rhs_ceiling,
                },
            ) => Self::at_least(
                lhs_minimum.saturating_mul(rhs_minimum),
                lhs_ceiling + rhs_ceiling,
            ),
            _ => Self::Unknown,
        }
    }

    /// The range of `shl(shift, value)` for `value` in `self`.
    fn shifted_left(self, shift: u64) -> Self {
        match (self, u32::try_from(shift)) {
            (
                Self::AtLeast {
                    minimum,
                    bit_ceiling,
                },
                Ok(shift),
            ) => Self::at_least(
                minimum.saturating_mul(2u64.saturating_pow(shift)),
                bit_ceiling.saturating_add(shift),
            ),
            _ => Self::Unknown,
        }
    }

    /// The range of a loop counter seeded in `self` that only steps up by static steps.
    ///
    /// A loop runs fewer than 2^64 steps (gas keeps it far below that), each below 2^64 because
    /// a static step is a `u64`, so the counter exceeds its seed by less than 2^(2 * 64).
    fn stepped_up(self) -> Self {
        match self {
            Self::AtLeast {
                minimum,
                bit_ceiling,
            } => Self::at_least(minimum, bit_ceiling.max(2 * u64::BITS) + 1),
            Self::Unknown => Self::Unknown,
        }
    }
}

/// Free-memory-pointer corruption state at the exits of a statement sequence scanned by
/// [`HeapAnalysis::scan_fmp_corruption`]. "Corrupt" always means: the FMP word `[0x40, 0x60)`
/// may hold arbitrary bytes written by an overlapping store.
///
/// A sequence can be left by falling through to the next statement, but also by edges that
/// skip intervening statements without terminating the frame: `break` (skips the loop `post`),
/// `continue` (jumps to the loop `post`), and `leave` (returns to the caller). Each such edge
/// carries its own corruption state, which the responsible enclosing construct — the `For`
/// handler for `break`/`continue`, the per-function summary for `leave` — merges at the edge's
/// actual target. Collapsing them into one state would let a path that re-establishes the
/// pointer mask a corrupted path that skips it.
#[derive(Clone, Copy, Default)]
struct FmpCorruptionExit {
    /// Corruption state at the fall-through exit. Meaningless when `terminates` is set.
    fallthrough_corrupt: bool,
    /// Whether every path leaves the sequence early (frame terminator, `leave`, `break`,
    /// `continue`), so control never falls through to a following statement.
    terminates: bool,
    /// Union of the corruption states at every `break` targeting the innermost enclosing loop
    /// (a nested `For` consumes its own body's `break` states).
    break_corrupt: bool,
    /// Union of the corruption states at every `continue` targeting the innermost enclosing
    /// loop.
    continue_corrupt: bool,
    /// Union of the corruption states at every `leave` (function return) in the sequence.
    leave_corrupt: bool,
}

impl FmpCorruptionExit {
    /// Folds a nested region's loop and function edge states (`break`/`continue`/`leave`) into
    /// this sequence's exit; the fall-through and termination states are merged by the caller
    /// according to the construct's control flow.
    fn absorb_jump_exits(&mut self, nested: FmpCorruptionExit) {
        self.break_corrupt |= nested.break_corrupt;
        self.continue_corrupt |= nested.continue_corrupt;
        self.leave_corrupt |= nested.leave_corrupt;
    }
}

/// One pass over a loop's per-iteration sequence for
/// [`HeapAnalysis::scan_loop_fmp_corruption`].
struct LoopIterationFmpCorruption {
    /// Corruption state after the loop exits: the condition-false fall-through or any `break`.
    exit_corrupt: bool,
    /// Corruption state at the next iteration's entry (after `post`, including `continue`
    /// paths).
    next_iteration_corrupt: bool,
}

impl HeapAnalysis {
    /// Creates a new heap analysis context for the given EVM heap size in bytes.
    pub fn new(heap_size: u64) -> Self {
        HeapAnalysis {
            heap_size,
            memory_accesses: BTreeMap::new(),
            offset_values: BTreeMap::new(),
            tainted_regions: BTreeSet::new(),
            escaping_regions: BTreeSet::new(),
            has_dynamic_escapes: false,
            min_dynamic_escape_start: None,
            has_dynamic_accesses: false,
            fmp_word_escapes: false,
            variable_accessed_offsets: BTreeSet::new(),
            fmp_could_be_unbounded: false,
            value_expressions: BTreeMap::new(),
            fmp_corrupting_functions: BTreeSet::new(),
            deferred_write_destinations: Vec::new(),
            free_pointer_relative_parameters: BTreeSet::new(),
            free_pointer_relative_returns: BTreeSet::new(),
            free_pointer_relative_merges: BTreeSet::new(),
        }
    }

    /// Runs heap analysis on an object.
    pub fn analyze_object(&mut self, object: &Object) {
        self.analyze_object_inner(object, true);
    }

    /// `is_root` distinguishes the top-level deploy object from runtime
    /// subobjects. The deploy object's tail `return(0, codesize)` returns the
    /// runtime code (raw bytes from `codecopy`), so its coverage of the FMP slot
    /// is not an observable BE-encoded escape and must not pessimize FMP native
    /// mode. A runtime subobject's top-level `return`, however, returns
    /// caller-observable data, so a return covering 0x40 there is a genuine FMP
    /// escape.
    fn analyze_object_inner(&mut self, object: &Object, is_root: bool) {
        self.analyze_block(&object.code, false, is_root);

        for function in object.functions.values() {
            self.analyze_block(&function.body, true, is_root);
        }

        self.check_deferred_write_destinations(object);
        self.detect_observed_fmp_corruption(object);

        for subobject in &object.subobjects {
            self.offset_values.clear();
            self.value_expressions.clear();
            self.analyze_object_inner(subobject, false);
        }

        self.compute_tainted_regions();
    }

    /// Analyzes a block for memory access patterns. Recursion through nested
    /// regions is handled by `for_each_statement`; `analyze_statement` only handles
    /// the per-statement analysis (no longer recursing internally).
    fn analyze_block(&mut self, block: &Block, in_function: bool, is_root: bool) {
        for_each_statement(&block.statements, &mut |statement| {
            self.analyze_statement(statement, in_function, is_root);
        });
    }

    /// Analyzes a single statement for memory access patterns. The caller is
    /// responsible for walking nested regions (use `for_each_statement`).
    fn analyze_statement(&mut self, statement: &Statement, in_function: bool, is_root: bool) {
        match statement {
            Statement::Let { bindings, value } => {
                if let Some(offset_info) = self.analyze_expression_offset(value) {
                    for binding in bindings {
                        self.offset_values.insert(binding.0, offset_info.clone());
                    }
                }
                if bindings.len() == 1 {
                    self.value_expressions.insert(bindings[0].0, value.clone());
                }
                self.analyze_expression_side_effects(value);
            }

            Statement::MStore {
                offset,
                value,
                region,
            } => {
                let pattern = self.classify_access(offset);
                self.track_variable_access(offset);
                let static_offset = self.extract_static_offset(offset);
                if let Some(address) = static_offset {
                    self.memory_accesses.insert(address, pattern);
                } else {
                    self.has_dynamic_accesses = true;
                }
                if !pattern.is_aligned() {
                    if let Some(address) = static_offset {
                        if address % BYTE_LENGTH_WORD as u64 != 0 {
                            self.taint_unaligned_access(address);
                        }
                    }
                }
                let is_fmp_store = region.is_free_pointer_slot(static_offset);
                if is_fmp_store && !self.is_trusted_fmp_source(value.id.0) {
                    self.fmp_could_be_unbounded = true;
                }
                if static_offset.is_none()
                    && (*region == MemoryRegion::Scratch
                        || self.loop_offset_may_reach_fmp_word(offset))
                {
                    self.fmp_could_be_unbounded = true;
                }
            }

            Statement::MStore8 { offset, .. } => {
                let pattern = AccessPattern::Unknown;
                if let Some(address) = self.extract_static_offset(offset) {
                    self.memory_accesses.insert(address, pattern);
                    self.tainted_regions.insert(word_align(address));
                    if word_align(address) == 0x40 {
                        self.fmp_could_be_unbounded = true;
                    }
                } else {
                    self.has_dynamic_accesses = true;
                    self.fmp_could_be_unbounded = true;
                }
            }

            Statement::MCopy {
                destination,
                source,
                length,
            } => {
                let destination_start = self.extract_static_offset(destination);
                let source_start = self.extract_static_offset(source);
                let len = self.extract_static_offset(length);
                self.flag_write_covering_fmp(destination, length, true);
                self.taint_range(destination_start, len);
                self.taint_range(source_start, len);
            }

            Statement::ExternalCall {
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                ..
            } => {
                self.mark_escaping_range(args_offset, args_length);
                self.note_fmp_coverage(args_offset, args_length);
                self.flag_write_covering_fmp(ret_offset, ret_length, true);
                self.mark_escaping_and_tainted_range(ret_offset, ret_length);
                self.note_fmp_coverage(ret_offset, ret_length);
            }

            Statement::Revert { offset, length } => {
                self.mark_escaping_range(offset, length);
                self.note_fmp_coverage(offset, length);
            }

            Statement::Return { offset, length } => {
                self.mark_escaping_range(offset, length);
                if in_function || !is_root {
                    self.note_fmp_coverage(offset, length);
                }
            }

            Statement::Log { offset, length, .. } => {
                self.mark_escaping_range(offset, length);
                self.note_fmp_coverage(offset, length);
            }

            Statement::Create { offset, length, .. } => {
                self.mark_escaping_range(offset, length);
                self.note_fmp_coverage(offset, length);
            }

            Statement::If { .. } | Statement::Switch { .. } | Statement::Block(_) => {}

            Statement::For {
                initial_values,
                loop_variables,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
                ..
            } => {
                self.record_loop_variable_ranges(
                    initial_values,
                    loop_variables,
                    body,
                    post_input_variables,
                    post,
                    outputs,
                );
                self.analyze_expression_side_effects(condition);
            }

            Statement::Expression(expression) => {
                self.analyze_expression_side_effects(expression);
            }

            Statement::ReturnDataCopy {
                destination,
                length,
                ..
            } => {
                self.taint_copy_destination(destination, length);
            }

            Statement::CodeCopy {
                destination,
                length,
                ..
            }
            | Statement::ExtCodeCopy {
                destination,
                length,
                ..
            }
            | Statement::DataCopy {
                destination,
                length,
                ..
            }
            | Statement::CallDataCopy {
                destination,
                length,
                ..
            } => {
                if let Some(address) = self.extract_static_offset(destination) {
                    self.memory_accesses
                        .entry(address)
                        .or_insert(AccessPattern::AlignedStatic(address));
                }
                self.taint_copy_destination(destination, length);
            }

            Statement::SStore { .. }
            | Statement::TStore { .. }
            | Statement::MappingSStore { .. }
            | Statement::SelfDestruct { .. }
            | Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::Leave { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. }
            | Statement::SetImmutable { .. } => {}
        }
    }

    /// Records the iteration ranges of a loop's variables whose seeds are computed from literals
    /// and literal-seeded loop counters, and of their `post` inputs and loop outputs. A variable
    /// gets its seed's range stepped up when the body's yield, every `continue` and `post` hand it
    /// on unchanged or plus a static step, and `Unknown` otherwise; its output keeps that range
    /// only when every `break` hands it on the same way, since a stepped-up value is one more such step.
    fn record_loop_variable_ranges(
        &mut self,
        initial_values: &[Value],
        loop_variables: &[crate::ir::ValueId],
        body: &crate::ir::Region,
        post_input_variables: &[crate::ir::ValueId],
        post: &crate::ir::Region,
        outputs: &[crate::ir::ValueId],
    ) {
        let mut definitions = BTreeMap::new();
        collect_loop_definitions(&body.statements, &mut definitions);
        collect_loop_definitions(&post.statements, &mut definitions);
        let mut breaks = Vec::new();
        let mut continues = Vec::new();
        crate::type_inference::collect_loop_control_values(
            &body.statements,
            &mut breaks,
            &mut continues,
        );
        for (index, initial_value) in initial_values.iter().enumerate() {
            let Some(seed) = self
                .offset_values
                .get(&initial_value.id.0)
                .and_then(|info| info.iteration_range)
            else {
                continue;
            };
            let counter = loop_variables[index];
            let post_input = post_input_variables[index];
            let steps_up = |handed: Option<&Value>, base| self.steps_up(handed, base, &definitions);
            let ascends = steps_up(body.yields.get(index), counter)
                && continues
                    .iter()
                    .all(|values| steps_up(values.get(index), counter))
                && steps_up(post.yields.get(index), post_input);
            let counter_range = if ascends {
                seed.stepped_up()
            } else {
                IterationRange::Unknown
            };
            let output_range = if breaks
                .iter()
                .all(|values| steps_up(values.get(index), counter))
            {
                counter_range
            } else {
                IterationRange::Unknown
            };
            for (value, range) in [
                (counter, counter_range),
                (post_input, counter_range),
                (outputs[index], output_range),
            ] {
                self.offset_values.insert(
                    value.0,
                    OffsetInfo {
                        iteration_range: Some(range),
                        ..OffsetInfo::default()
                    },
                );
            }
        }
    }

    /// Classifies a memory access based on the offset value.
    fn classify_access(&self, offset: &Value) -> AccessPattern {
        if let Some(info) = self.offset_values.get(&offset.id.0) {
            if let Some(static_value) = info.static_value {
                if static_value % BYTE_LENGTH_WORD as u64 == 0 {
                    return AccessPattern::AlignedStatic(static_value);
                } else {
                    return AccessPattern::UnalignedStatic(static_value);
                }
            }
            if info.alignment >= 32 {
                return AccessPattern::AlignedDynamic;
            }
        }
        AccessPattern::Unknown
    }

    /// Extracts a static offset value if known.
    fn extract_static_offset(&self, offset: &Value) -> Option<u64> {
        self.offset_values
            .get(&offset.id.0)
            .and_then(|info| info.static_value)
    }

    /// Records that a static offset was accessed via a non-literal expression.
    /// This means LLVM may not see it as a constant, causing a mode mismatch
    /// if we use native mode for literal accesses to the same offset.
    fn track_variable_access(&mut self, offset: &Value) {
        if let Some(info) = self.offset_values.get(&offset.id.0) {
            if let Some(static_value) = info.static_value {
                if !info.from_literal {
                    self.variable_accessed_offsets.insert(static_value);
                }
            }
        }
    }

    /// Marks all word-aligned memory regions in [offset, offset+length) as escaping.
    /// Notes a static escape range that may cover the FMP word at 0x40.
    /// Each external escape statement (`revert`, `return` in a function,
    /// `log*`, external `call`, `create*`, `keccak256`) must call this so
    /// `fmp_native_safe()` can disable the FMP native-mode encoding when
    /// the FMP value would be observed externally in BE format.
    ///
    /// - `(static_start, static_len)` covering `[0x40, 0x60)` → set
    ///   `fmp_word_escapes`.
    /// - `(static_start, dynamic_len)` with `start <= 0x40` → could cover
    ///   FMP, set `min_dynamic_escape_start` to that word.
    /// - `(dynamic, _)` → offset is unknown so the escape could start
    ///   anywhere including at/below 0x40; lower `min_dynamic_escape_start`
    ///   to 0 so `fmp_native_safe()` rejects.
    fn note_fmp_coverage(&mut self, offset: &Value, length: &Value) {
        let start = self.extract_static_offset(offset);
        let len = self.extract_static_offset(length);
        match (start, len) {
            (Some(s), Some(l)) => {
                if s <= 0x40 && s.saturating_add(l) >= 0x60 {
                    self.fmp_word_escapes = true;
                }
            }
            (Some(s), None) => {
                let word_start = word_align(s);
                self.min_dynamic_escape_start = Some(
                    self.min_dynamic_escape_start
                        .map_or(word_start, |previous| previous.min(word_start)),
                );
            }
            (None, _) => {
                self.min_dynamic_escape_start = Some(0);
            }
        }
    }

    fn mark_escaping_range(&mut self, offset: &Value, length: &Value) {
        let start = self.extract_static_offset(offset);
        let len = self.extract_static_offset(length);
        match (start, len) {
            (Some(_), Some(0)) => {}
            (Some(address), Some(size)) => {
                let end = address.saturating_add(size);
                let first_word = word_align(address);
                let range = end.saturating_sub(first_word);
                let num_words =
                    range.saturating_add(BYTE_LENGTH_WORD as u64 - 1) / BYTE_LENGTH_WORD as u64;
                if num_words > MAX_RANGE_WORDS {
                    self.escaping_regions.insert(first_word);
                    self.has_dynamic_escapes = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.escaping_regions.insert(word);
                        word += BYTE_LENGTH_WORD as u64;
                    }
                }
            }
            (Some(address), None) => {
                self.escaping_regions.insert(word_align(address));
                self.has_dynamic_escapes = true;
            }
            (None, _) => {
                self.has_dynamic_escapes = true;
            }
        }
    }

    /// Flags the free memory pointer as possibly unbounded when a raw write of `length`
    /// bytes to `destination` can cover `[0x40, 0x60)`. A zero length never covers it,
    /// and neither does a dynamic destination that is free pointer relative, or a
    /// loop-varying destination that never drops below `0x60`. A static destination below
    /// `0x60` with a dynamic length is flagged when `any_dynamic_length` is set or the length
    /// is loop-varying. Copy opcodes leave it unset, so `calldatacopy(0, 0, calldatasize())`
    /// is not flagged although a length above `0x40` overwrites the pointer (a known gap).
    fn flag_write_covering_fmp(
        &mut self,
        destination: &Value,
        length: &Value,
        any_dynamic_length: bool,
    ) {
        let destination_start = self.extract_static_offset(destination);
        let len = self.extract_static_offset(length);
        let dynamic_length_covers = any_dynamic_length || self.loop_varying(length);

        let covers_fmp = match (destination_start, len) {
            (_, Some(0)) => false,
            (Some(address), Some(size)) => address < 0x60 && address.saturating_add(size) > 0x40,
            (Some(address), None) => {
                (0x40..0x60).contains(&address) || (dynamic_length_covers && address < 0x60)
            }
            (None, _) if self.loop_varying(destination) => {
                self.loop_offset_may_reach_fmp_word(destination)
            }
            (None, _) => {
                if !self.is_free_pointer_relative(destination.id.0) {
                    self.deferred_write_destinations.push(destination.id.0);
                }
                false
            }
        };
        if covers_fmp {
            self.fmp_could_be_unbounded = true;
        }
    }

    /// Flags the free memory pointer as possibly unbounded when a deferred write destination is
    /// not free pointer relative, even when trusting the parameters, call results and control
    /// flow merges for which every incoming value is. The trusted sets start full and shrink to
    /// a fixed point, so a pointer carried around a loop or passed along by a recursive call
    /// stays trusted when every value entering it from outside is.
    fn check_deferred_write_destinations(&mut self, object: &Object) {
        if self.deferred_write_destinations.is_empty() {
            return;
        }

        let mut call_sites = Vec::new();
        let mut collect = |statement: &Statement| {
            let expression = match statement {
                Statement::Let { value, .. } | Statement::Expression(value) => value,
                Statement::For { condition, .. } => condition,
                _ => return,
            };
            if let Expression::Call {
                function,
                arguments,
            } = expression
            {
                call_sites.push((*function, arguments.clone()));
            }
        };
        for_each_statement(&object.code.statements, &mut collect);
        for function in object.functions.values() {
            for_each_statement(&function.body.statements, &mut collect);
        }

        let mut merge_inputs = BTreeMap::new();
        collect_merge_inputs(
            &object.code.statements,
            None,
            &mut merge_inputs,
            &mut Vec::new(),
        );
        let mut returned_values = BTreeMap::new();
        for function in object.functions.values() {
            let mut leave_values = Vec::new();
            collect_merge_inputs(
                &function.body.statements,
                None,
                &mut merge_inputs,
                &mut leave_values,
            );
            if let [return_value] = function.return_values.as_slice() {
                leave_values.push(return_value.0);
                returned_values.insert(function.id, leave_values);
            }
        }

        let called: BTreeSet<FunctionId> = call_sites.iter().map(|(id, _)| *id).collect();
        for function in object.functions.values() {
            if called.contains(&function.id) {
                self.free_pointer_relative_parameters
                    .extend(function.parameters.iter().map(|(id, _)| id.0));
            }
        }
        self.free_pointer_relative_returns
            .extend(returned_values.keys().copied());
        self.free_pointer_relative_merges
            .extend(merge_inputs.keys().copied());

        loop {
            let mut changed = false;
            for (function_id, arguments) in &call_sites {
                let Some(function) = object.functions.get(function_id) else {
                    continue;
                };
                for ((parameter, _), argument) in function.parameters.iter().zip(arguments) {
                    if self.free_pointer_relative_parameters.contains(&parameter.0)
                        && !self.is_free_pointer_relative(argument.id.0)
                    {
                        self.free_pointer_relative_parameters.remove(&parameter.0);
                        changed = true;
                    }
                }
            }
            for (function_id, values) in &returned_values {
                if self.free_pointer_relative_returns.contains(function_id)
                    && !values
                        .iter()
                        .all(|value| self.is_free_pointer_relative(*value))
                {
                    self.free_pointer_relative_returns.remove(function_id);
                    changed = true;
                }
            }
            for (merge, inputs) in &merge_inputs {
                if self.free_pointer_relative_merges.contains(merge)
                    && !inputs
                        .iter()
                        .all(|input| self.is_free_pointer_relative(*input))
                {
                    self.free_pointer_relative_merges.remove(merge);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let deferred = std::mem::take(&mut self.deferred_write_destinations);
        if deferred
            .iter()
            .any(|destination| !self.is_free_pointer_relative(*destination))
        {
            self.fmp_could_be_unbounded = true;
        }
        self.free_pointer_relative_parameters.clear();
        self.free_pointer_relative_returns.clear();
        self.free_pointer_relative_merges.clear();
    }

    /// Taints the destination of a copy opcode (`calldatacopy`, `codecopy`,
    /// `returndatacopy`, …), which writes big-endian bytes that a later native
    /// (little-endian) `mload` must not byte-reverse.
    ///
    /// When the length is statically known, every word the copy covers is tainted
    /// — not just the start word — so a multi-word copy can't leave a later word a
    /// native candidate. A dynamic length taints only the start word. A loop-varying
    /// length also sets `has_dynamic_accesses`, because later iterations copy past it;
    /// any other dynamic length does not, which keeps native mode for every ABI-decode
    /// `calldatacopy` but leaves a later literal word native (a known gap).
    fn taint_copy_destination(&mut self, destination: &Value, length: &Value) {
        let destination_start = self.extract_static_offset(destination);
        let len = self.extract_static_offset(length);

        let loop_length = self.loop_varying(length);
        self.flag_write_covering_fmp(destination, length, false);

        match (destination_start, len) {
            (Some(address), Some(size)) => {
                if size > 0 {
                    self.taint_range(Some(address), Some(size));
                }
            }
            (Some(address), None) => {
                self.tainted_regions.insert(word_align(address));
                if loop_length {
                    self.has_dynamic_accesses = true;
                }
            }
            (None, _) => self.has_dynamic_accesses = true,
        }
    }

    /// Whether `value` is computed only from literals and literal-seeded loop counters but has no
    /// static value, such as a loop counter or an offset computed from one.
    fn loop_varying(&self, value: &Value) -> bool {
        self.offset_values
            .get(&value.id.0)
            .is_some_and(|info| info.static_value.is_none() && info.iteration_range.is_some())
    }

    /// Whether a word store or copy through a loop-varying `offset` could overlap the FMP word.
    fn loop_offset_may_reach_fmp_word(&self, offset: &Value) -> bool {
        self.loop_varying(offset) && !self.loop_offset_at_or_above_fmp_word_end(offset)
    }

    /// Whether a loop-varying `offset` never drops below the end of the FMP word.
    fn loop_offset_at_or_above_fmp_word_end(&self, offset: &Value) -> bool {
        self.loop_varying(offset)
            && self
                .offset_values
                .get(&offset.id.0)
                .and_then(|info| info.iteration_range)
                .is_some_and(|range| match range {
                    IterationRange::AtLeast { minimum, .. } => minimum >= 0x60,
                    IterationRange::Unknown => false,
                })
    }

    /// Whether `handed`, a value that a loop hands to its next iteration or to its outputs, is
    /// `base` itself or `add(base, step)` with a static `step`, in either operand order.
    /// `definitions` holds the loop's own bindings, which the analysis has not reached yet.
    fn steps_up(
        &self,
        handed: Option<&Value>,
        base: crate::ir::ValueId,
        definitions: &BTreeMap<crate::ir::ValueId, &Expression>,
    ) -> bool {
        let Some(handed) = handed else {
            return false;
        };
        if handed.id == base {
            return true;
        }
        let Some(Expression::Binary {
            operation: crate::ir::BinaryOperation::Add,
            lhs,
            rhs,
        }) = definitions.get(&handed.id)
        else {
            return false;
        };
        let is_static_step = |step: &Value| match definitions.get(&step.id) {
            Some(expression) => self
                .analyze_expression_offset(expression)
                .is_some_and(|info| info.static_value.is_some()),
            None => self.extract_static_offset(step).is_some(),
        };
        (lhs.id == base && is_static_step(rhs)) || (rhs.id == base && is_static_step(lhs))
    }

    /// Whether a dynamic memory destination is a free-memory-pointer-relative address, and so
    /// provably cannot land on the FMP word `[0x40, 0x60)`.
    ///
    /// A copy to a dynamic destination (`calldatacopy(dst, _, _)` etc.) corrupts the FMP when `dst`
    /// can equal an offset in `[0x40, 0x60)`. solc only ever copies to an FMP-relative address
    /// (`add(mload(0x40), k)`), which is `>= 0x80` because the bounded FMP is `>= 0x80` — so it
    /// cannot hit the slot. A destination that is *not* recognizably FMP-relative (e.g.
    /// `and(calldataload(p), 0xff)`, which ranges over `[0, 0xff]`) can, and must flag
    /// `fmp_could_be_unbounded` so the corrupted `mload(0x40)` skips the `FMP < heap_size` range
    /// proof. Recognized as at-or-above the free pointer: `mload(0x40)`, a literal base `>= 0x60`
    /// that fits in 64 bits (`mem_opt` constant-forwards `mload(0x40)` to its `0x80` literal), an
    /// `add` with such an operand, `Var` forwarding chains, and the parameters and call results
    /// trusted by [`Self::check_deferred_write_destinations`]. (The adversarial `add(mload(0x40),
    /// k)` / `add(0x80, k)` that wraps mod 2^256 back to `0x40` is the same solc-unreachable residual
    /// as the dynamic full-word `MStore` gap; see the `fmp_could_be_unbounded` field docs.)
    fn is_free_pointer_relative(&self, value_id: u32) -> bool {
        const MAX_DEPTH: u32 = 32;
        let mut current = value_id;
        for _ in 0..MAX_DEPTH {
            match self.value_expressions.get(&current) {
                None => {
                    return self.free_pointer_relative_parameters.contains(&current)
                        || self.free_pointer_relative_merges.contains(&current);
                }
                Some(Expression::Call { function, .. }) => {
                    return self.free_pointer_relative_returns.contains(function);
                }
                Some(Expression::MLoad { offset, .. }) => {
                    return self.extract_static_offset(offset) == Some(0x40);
                }
                Some(Expression::Literal { value, .. }) => {
                    let digits = value.to_u64_digits();
                    return digits.len() <= 1 && digits.first().copied().unwrap_or(0) >= 0x60;
                }
                Some(Expression::Var(inner)) => current = inner.0,
                Some(Expression::Binary {
                    operation: crate::ir::BinaryOperation::Add,
                    lhs,
                    rhs,
                }) => {
                    return self.is_free_pointer_relative(lhs.id.0)
                        || self.is_free_pointer_relative(rhs.id.0);
                }
                _ => return false,
            }
        }
        false
    }

    /// Taints every word a static, non-word-aligned full-word access (`mload`/`mstore`) covers.
    ///
    /// EVM memory is big-endian; the native-mode optimization keeps a word's bytes little-endian
    /// (skipping the byte-swap) only when *every* access to that word is word-aligned. An unaligned
    /// full-word access at `address` reads or writes the raw bytes of `[address, address + 32)`,
    /// spanning the two words `word_align(address)` and `word_align(address) + 32`. If either word
    /// were left a native candidate, an aligned native (little-endian) access to it and this
    /// unaligned (big-endian) access would disagree on byte order for the overlapping bytes,
    /// byte-swapping the result. Tainting both covered words forces them big-endian so all accesses
    /// agree. `mstore(0x20, w); r := mload(0x08)` is the canonical trigger: the aligned store is
    /// native but the unaligned load reads word `0x20` in the wrong order without this taint.
    fn taint_unaligned_access(&mut self, address: u64) {
        self.taint_range(Some(address), Some(BYTE_LENGTH_WORD as u64));
    }

    /// Taints all word-aligned memory regions in a range.
    /// If the range is too large, treats it as a dynamic access instead.
    fn taint_range(&mut self, start: Option<u64>, len: Option<u64>) {
        match (start, len) {
            (Some(address), Some(size)) if size > 0 => {
                let end = address.saturating_add(size);
                let first_word = word_align(address);
                let num_words = end
                    .saturating_sub(first_word)
                    .saturating_add(BYTE_LENGTH_WORD as u64 - 1)
                    / BYTE_LENGTH_WORD as u64;
                if num_words > MAX_RANGE_WORDS {
                    self.tainted_regions.insert(first_word);
                    self.has_dynamic_accesses = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.tainted_regions.insert(word);
                        word += BYTE_LENGTH_WORD as u64;
                    }
                }
            }
            (Some(address), _) => {
                self.tainted_regions.insert(word_align(address));
                self.has_dynamic_accesses = true;
            }
            (None, _) => {
                self.has_dynamic_accesses = true;
            }
        }
    }

    /// Sets `fmp_could_be_unbounded` when an unaligned store that corrupts the free-memory-pointer
    /// word `[0x40, 0x60)` is *observed* — i.e. an `mload(0x40)` (the range-proof consumer) is
    /// reachable after it before the frame terminates.
    ///
    /// An unaligned full-word store at `o ∈ [0x21, 0x5f] \ {0x40}` overwrites part of the FMP with
    /// arbitrary bytes ([`crate::ir::word_store_overlaps_free_pointer_slot`]). If a later
    /// `mload(0x40)` reads the corrupted pointer, codegen's `FMP < heap_size` range proof (gated on
    /// `fmp_could_be_unbounded`, see the field docs) would truncate the arbitrary value into range,
    /// so a store through it silently succeeds where EVM runs out of gas.
    ///
    /// Flagging *every* such store would disable the FMP optimization for almost every contract:
    /// solc's `Error`/`Panic`/custom-error and multi-value return ABI encoders write `mstore(0x24,
    /// _)` / `mstore(0x44, _)` into scratch and then immediately `revert`/`return`, discarding the
    /// FMP unobserved (~+5% / +17 KB on the OZ corpus). This control-flow-aware pass distinguishes
    /// the two: it walks the structured statement tree tracking whether the FMP word may currently
    /// hold arbitrary bytes (`corrupt`), and flags only when an FMP read is reachable while
    /// `corrupt` holds. A frame terminator (`revert`/`return`/`stop`/`invalid`/panic/error) ends
    /// the path, so corruption built purely to feed a terminator is never flagged; an aligned
    /// `mstore(0x40, _)` overwrites the whole word and re-establishes a defined pointer.
    ///
    /// Corruption also propagates across the edges that leave a statement sequence without
    /// terminating the frame ([`FmpCorruptionExit`]): a `break` carries its state past the loop
    /// `post` to the statement after the loop, a `continue` carries it into `post`, and a `leave`
    /// (or a function-body fall-through) carries it back to every caller. The latter is modeled
    /// interprocedurally: function summaries (`fmp_corrupting_functions`) are computed to a fixed
    /// point over the object's call graph — the summary set only grows, so iteration terminates —
    /// and a call to a summarized function corrupts at the call site. Runs after `analyze_block`
    /// so offset resolution (`offset_values`) for this object is fully populated.
    fn detect_observed_fmp_corruption(&mut self, object: &Object) {
        self.fmp_corrupting_functions.clear();
        loop {
            let mut summaries_changed = false;
            for function in object.functions.values() {
                let exit = self.scan_fmp_corruption(&function.body.statements, false);
                let exits_corrupt =
                    exit.leave_corrupt || (!exit.terminates && exit.fallthrough_corrupt);
                if exits_corrupt && self.fmp_corrupting_functions.insert(function.id) {
                    summaries_changed = true;
                }
            }
            if !summaries_changed {
                break;
            }
        }
        self.scan_fmp_corruption(&object.code.statements, false);
    }

    /// Walks `statements` in program order for [`Self::detect_observed_fmp_corruption`],
    /// tracking whether the FMP word may hold arbitrary bytes (`corrupt`) and setting
    /// `fmp_could_be_unbounded` on an observed read while `corrupt` holds.
    ///
    /// `ErrorStringRevert` observes before terminating: its outlined `Error(string)` helper
    /// reads `mload(0x40)` for its encoding buffer.
    fn scan_fmp_corruption(
        &mut self,
        statements: &[Statement],
        corrupt_in: bool,
    ) -> FmpCorruptionExit {
        let mut corrupt = corrupt_in;
        let mut exit = FmpCorruptionExit::default();
        for statement in statements {
            match statement {
                Statement::Let { value, .. } => {
                    corrupt = self.scan_expression_fmp_corruption(value, corrupt);
                }
                Statement::Expression(expression) => {
                    corrupt = self.scan_expression_fmp_corruption(expression, corrupt);
                }
                Statement::MStore { offset, region, .. } => {
                    let static_offset = self.extract_static_offset(offset);
                    if region.is_free_pointer_slot(static_offset) {
                        corrupt = false;
                    } else if crate::ir::word_store_overlaps_free_pointer_slot(static_offset) {
                        corrupt = true;
                    }
                }
                Statement::MStore8 { offset, .. } => {
                    if let Some(address) = self.extract_static_offset(offset) {
                        if (0x40..0x60).contains(&address) {
                            corrupt = true;
                        }
                    }
                }
                Statement::If {
                    then_region,
                    else_region,
                    ..
                } => {
                    let then_exit = self.scan_fmp_corruption(&then_region.statements, corrupt);
                    let else_exit = match else_region {
                        Some(region) => self.scan_fmp_corruption(&region.statements, corrupt),
                        None => FmpCorruptionExit {
                            fallthrough_corrupt: corrupt,
                            ..FmpCorruptionExit::default()
                        },
                    };
                    exit.absorb_jump_exits(then_exit);
                    exit.absorb_jump_exits(else_exit);
                    corrupt = (!then_exit.terminates && then_exit.fallthrough_corrupt)
                        || (!else_exit.terminates && else_exit.fallthrough_corrupt);
                    if then_exit.terminates && else_exit.terminates {
                        exit.terminates = true;
                        return exit;
                    }
                }
                Statement::Switch { cases, default, .. } => {
                    let mut fallthrough_corrupt = false;
                    let mut all_terminate = default.is_some();
                    for case in cases {
                        let case_exit = self.scan_fmp_corruption(&case.body.statements, corrupt);
                        exit.absorb_jump_exits(case_exit);
                        fallthrough_corrupt |=
                            !case_exit.terminates && case_exit.fallthrough_corrupt;
                        all_terminate &= case_exit.terminates;
                    }
                    match default {
                        Some(region) => {
                            let default_exit =
                                self.scan_fmp_corruption(&region.statements, corrupt);
                            exit.absorb_jump_exits(default_exit);
                            fallthrough_corrupt |=
                                !default_exit.terminates && default_exit.fallthrough_corrupt;
                            all_terminate &= default_exit.terminates;
                        }
                        None => fallthrough_corrupt |= corrupt,
                    }
                    corrupt = fallthrough_corrupt;
                    if all_terminate {
                        exit.terminates = true;
                        return exit;
                    }
                }
                Statement::For {
                    condition_statements,
                    condition,
                    body,
                    post,
                    ..
                } => {
                    corrupt = self.scan_loop_fmp_corruption(
                        condition_statements,
                        condition,
                        &body.statements,
                        &post.statements,
                        corrupt,
                        &mut exit,
                    );
                }
                Statement::Block(block) => {
                    let block_exit = self.scan_fmp_corruption(&block.statements, corrupt);
                    exit.absorb_jump_exits(block_exit);
                    corrupt = !block_exit.terminates && block_exit.fallthrough_corrupt;
                    if block_exit.terminates {
                        exit.terminates = true;
                        return exit;
                    }
                }
                Statement::ErrorStringRevert { .. } => {
                    if corrupt {
                        self.fmp_could_be_unbounded = true;
                    }
                    exit.terminates = true;
                    return exit;
                }
                Statement::Leave { .. } => {
                    exit.leave_corrupt |= corrupt;
                    exit.terminates = true;
                    return exit;
                }
                Statement::Break { .. } => {
                    exit.break_corrupt |= corrupt;
                    exit.terminates = true;
                    return exit;
                }
                Statement::Continue { .. } => {
                    exit.continue_corrupt |= corrupt;
                    exit.terminates = true;
                    return exit;
                }
                Statement::Revert { .. }
                | Statement::Return { .. }
                | Statement::Stop
                | Statement::Invalid
                | Statement::SelfDestruct { .. }
                | Statement::PanicRevert { .. }
                | Statement::CustomErrorRevert { .. } => {
                    exit.terminates = true;
                    return exit;
                }
                _ => {}
            }
        }
        exit.fallthrough_corrupt = corrupt;
        exit
    }

    /// Flags an observed FMP read when `corrupt` holds, applies the callee corruption summary
    /// for calls, and returns the corruption state after evaluating `expression`.
    fn scan_expression_fmp_corruption(&mut self, expression: &Expression, corrupt: bool) -> bool {
        if corrupt && self.expression_observes_fmp(expression) {
            self.fmp_could_be_unbounded = true;
        }
        if let Expression::Call { function, .. } = expression {
            if self.fmp_corrupting_functions.contains(function) {
                return true;
            }
        }
        corrupt
    }

    /// Conservatively scans a `for` loop's per-iteration sequence to a fixed point.
    ///
    /// A store in one iteration can be observed by a read in a later iteration, so the
    /// per-iteration sequence (`condition_statements`, body, `post`) is re-scanned from a
    /// corrupted entry state when the sequence can produce corruption at the next iteration's
    /// entry. Corruption is a two-point lattice, so one re-scan reaches the fixed point.
    ///
    /// The loop is left either through the condition evaluating false (fall-through of
    /// `condition_statements` followed by the `condition` expression) or through a `break` — and a
    /// `break` jumps *past* `post`, so the returned exit state is the union of those two edges, not
    /// the state after `post` (a `post` that re-establishes the pointer must not mask a corrupted
    /// `break` path). `leave` states from inside the loop are folded into `enclosing_exit`.
    fn scan_loop_fmp_corruption(
        &mut self,
        condition_statements: &[Statement],
        condition: &Expression,
        body: &[Statement],
        post: &[Statement],
        corrupt_in: bool,
        enclosing_exit: &mut FmpCorruptionExit,
    ) -> bool {
        let first_pass = self.scan_loop_iteration_fmp_corruption(
            condition_statements,
            condition,
            body,
            post,
            corrupt_in,
            enclosing_exit,
        );
        if first_pass.next_iteration_corrupt && !corrupt_in {
            let second_pass = self.scan_loop_iteration_fmp_corruption(
                condition_statements,
                condition,
                body,
                post,
                true,
                enclosing_exit,
            );
            return second_pass.exit_corrupt;
        }
        first_pass.exit_corrupt
    }

    /// One pass over a loop's per-iteration sequence for [`Self::scan_loop_fmp_corruption`].
    fn scan_loop_iteration_fmp_corruption(
        &mut self,
        condition_statements: &[Statement],
        condition: &Expression,
        body: &[Statement],
        post: &[Statement],
        corrupt_in: bool,
        enclosing_exit: &mut FmpCorruptionExit,
    ) -> LoopIterationFmpCorruption {
        let condition_exit = self.scan_fmp_corruption(condition_statements, corrupt_in);
        let condition_corrupt =
            self.scan_expression_fmp_corruption(condition, condition_exit.fallthrough_corrupt);
        let body_exit = self.scan_fmp_corruption(body, condition_corrupt);
        let post_entry_corrupt =
            (!body_exit.terminates && body_exit.fallthrough_corrupt) || body_exit.continue_corrupt;
        let post_exit = self.scan_fmp_corruption(post, post_entry_corrupt);
        enclosing_exit.leave_corrupt |=
            condition_exit.leave_corrupt || body_exit.leave_corrupt || post_exit.leave_corrupt;
        LoopIterationFmpCorruption {
            exit_corrupt: condition_corrupt
                || condition_exit.break_corrupt
                || body_exit.break_corrupt
                || post_exit.break_corrupt,
            next_iteration_corrupt: (!post_exit.terminates && post_exit.fallthrough_corrupt)
                || post_exit.continue_corrupt,
        }
    }

    /// Whether evaluating `expression` reads the free-memory pointer in a way the `FMP < heap_size`
    /// range proof would mis-handle if the pointer is corrupted: a direct `mload(0x40)`, or a user
    /// function call whose body could read `mload(0x40)`.
    fn expression_observes_fmp(&self, expression: &Expression) -> bool {
        match expression {
            Expression::MLoad { offset, region } => {
                *region == MemoryRegion::FreePointerSlot
                    || self.extract_static_offset(offset) == Some(0x40)
            }
            Expression::Call { .. } => true,
            _ => false,
        }
    }

    /// Marks all word-aligned memory regions in a range as both escaping and tainted.
    fn mark_escaping_and_tainted_range(&mut self, offset: &Value, length: &Value) {
        let start = self.extract_static_offset(offset);
        let len = self.extract_static_offset(length);
        match (start, len) {
            (Some(address), Some(size)) if size > 0 => {
                let end = address.saturating_add(size);
                let first_word = word_align(address);
                let num_words = end
                    .saturating_sub(first_word)
                    .saturating_add(BYTE_LENGTH_WORD as u64 - 1)
                    / BYTE_LENGTH_WORD as u64;
                if num_words > MAX_RANGE_WORDS {
                    self.escaping_regions.insert(first_word);
                    self.tainted_regions.insert(first_word);
                    self.has_dynamic_escapes = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.escaping_regions.insert(word);
                        self.tainted_regions.insert(word);
                        word += BYTE_LENGTH_WORD as u64;
                    }
                }
            }
            (Some(address), None) => {
                self.escaping_regions.insert(word_align(address));
                self.tainted_regions.insert(word_align(address));
                self.has_dynamic_escapes = true;
            }
            _ => {
                self.has_dynamic_escapes = true;
            }
        }
    }

    /// Analyzes an expression to extract offset information.
    fn analyze_expression_offset(&self, expression: &Expression) -> Option<OffsetInfo> {
        match expression {
            Expression::Literal { value, .. } => {
                let digits = value.to_u64_digits();
                let static_value = if digits.is_empty() {
                    0
                } else if digits.len() == 1 {
                    digits[0]
                } else {
                    return Some(OffsetInfo {
                        from_literal: true,
                        iteration_range: Some(IterationRange::Unknown),
                        ..OffsetInfo::default()
                    });
                };
                Some(OffsetInfo {
                    static_value: Some(static_value),
                    alignment: compute_alignment(static_value),
                    from_literal: true,
                    iteration_range: Some(IterationRange::exactly(static_value)),
                })
            }

            Expression::Var(id) => self.offset_values.get(&id.0).cloned().map(|mut info| {
                info.from_literal = false;
                info
            }),

            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let lhs_info = self.offset_values.get(&lhs.id.0);
                let rhs_info = self.offset_values.get(&rhs.id.0);
                let iteration_range = match (
                    lhs_info.and_then(|info| info.iteration_range),
                    rhs_info.and_then(|info| info.iteration_range),
                ) {
                    (Some(lhs_range), Some(rhs_range)) => Some(match operation {
                        crate::ir::BinaryOperation::Add => lhs_range.plus(rhs_range),
                        crate::ir::BinaryOperation::Mul => lhs_range.times(rhs_range),
                        crate::ir::BinaryOperation::Shl => {
                            match lhs_info.and_then(|info| info.static_value) {
                                Some(shift) => rhs_range.shifted_left(shift),
                                None => IterationRange::Unknown,
                            }
                        }
                        _ => IterationRange::Unknown,
                    }),
                    _ => None,
                };

                let info = match operation {
                    crate::ir::BinaryOperation::Add => {
                        let lhs_align = lhs_info.map(|info| info.alignment).unwrap_or(1);
                        let rhs_align = rhs_info.map(|info| info.alignment).unwrap_or(1);
                        let result_align = gcd(lhs_align, rhs_align);

                        let static_value = match (
                            lhs_info.and_then(|info| info.static_value),
                            rhs_info.and_then(|info| info.static_value),
                        ) {
                            (Some(lhs_value), Some(rhs_value)) => {
                                Some(lhs_value.wrapping_add(rhs_value))
                            }
                            _ => None,
                        };

                        Some(OffsetInfo {
                            static_value,
                            alignment: result_align,
                            from_literal: false,
                            iteration_range,
                        })
                    }

                    crate::ir::BinaryOperation::Mul => {
                        let static_value = match (
                            lhs_info.and_then(|info| info.static_value),
                            rhs_info.and_then(|info| info.static_value),
                        ) {
                            (Some(lhs_value), Some(rhs_value)) => {
                                Some(lhs_value.wrapping_mul(rhs_value))
                            }
                            _ => None,
                        };

                        let mult_align = match (
                            rhs_info.and_then(|info| info.static_value),
                            lhs_info.and_then(|info| info.static_value),
                        ) {
                            (Some(32), _) | (_, Some(32)) => 32,
                            (Some(factor), _) | (_, Some(factor)) if factor % 32 == 0 => 32,
                            _ => 1,
                        };

                        Some(OffsetInfo {
                            static_value,
                            alignment: mult_align,
                            from_literal: false,
                            iteration_range,
                        })
                    }

                    crate::ir::BinaryOperation::And => {
                        if let Some(mask) = rhs_info.and_then(|info| info.static_value) {
                            let align = compute_alignment((!mask).wrapping_add(1));
                            Some(OffsetInfo {
                                static_value: None,
                                alignment: align.max(1),
                                from_literal: false,
                                iteration_range,
                            })
                        } else {
                            None
                        }
                    }

                    crate::ir::BinaryOperation::Shl => {
                        if let Some(shift) = lhs_info.and_then(|info| info.static_value) {
                            if shift < 32 {
                                let base_align = rhs_info.map(|info| info.alignment).unwrap_or(1);
                                Some(OffsetInfo {
                                    static_value: None,
                                    alignment: base_align.saturating_mul(1 << shift),
                                    from_literal: false,
                                    iteration_range,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }

                    _ => None,
                };
                info.or_else(|| {
                    iteration_range.map(|range| OffsetInfo {
                        iteration_range: Some(range),
                        ..OffsetInfo::default()
                    })
                })
            }

            Expression::Unary { operand, .. } => self.operation_without_minimum(&[operand]),

            Expression::Ternary { a, b, n, .. } => self.operation_without_minimum(&[a, b, n]),

            Expression::Truncate { value, .. }
            | Expression::ZeroExtend { value, .. }
            | Expression::SignExtendTo { value, .. } => self.operation_without_minimum(&[value]),

            Expression::MLoad { .. } => None,

            Expression::CallDataLoad { .. } => None,

            _ => None,
        }
    }

    /// The offset info of a pure operation that [`Self::analyze_expression_offset`] has no
    /// minimum rule for, such as `not`: computed from literals and literal-seeded loop counters,
    /// without a provable minimum, when all its `operands` are, and nothing otherwise.
    fn operation_without_minimum(&self, operands: &[&Value]) -> Option<OffsetInfo> {
        operands
            .iter()
            .all(|operand| {
                self.offset_values
                    .get(&operand.id.0)
                    .is_some_and(|info| info.iteration_range.is_some())
            })
            .then(|| OffsetInfo {
                iteration_range: Some(IterationRange::Unknown),
                ..OffsetInfo::default()
            })
    }

    /// Analyzes expression side effects on memory.
    fn analyze_expression_side_effects(&mut self, expression: &Expression) {
        match expression {
            Expression::MLoad { offset, .. } => {
                let _ = self.classify_access(offset);
                self.track_variable_access(offset);
                match self.extract_static_offset(offset) {
                    None => self.has_dynamic_accesses = true,
                    Some(address) if address % BYTE_LENGTH_WORD as u64 != 0 => {
                        self.taint_unaligned_access(address);
                    }
                    Some(_) => {}
                }
            }
            Expression::Keccak256 { offset, length } => {
                let _ = self.classify_access(offset);
                if self.extract_static_offset(offset).is_none() {
                    self.has_dynamic_accesses = true;
                }
                self.mark_escaping_range(offset, length);
                self.note_fmp_coverage(offset, length);
            }
            Expression::Keccak256Pair { .. } | Expression::Keccak256Single { .. } => {}
            Expression::MappingSLoad { .. } => {}
            _ => {}
        }
    }

    /// Computes which regions need big-endian emulation.
    fn compute_tainted_regions(&mut self) {
        for &region in &self.escaping_regions {
            self.tainted_regions.insert(region);
        }
    }

    /// Returns whether a memory region requires big-endian emulation.
    pub fn requires_big_endian(&self, address: u64) -> bool {
        let word_address = word_align(address);
        self.tainted_regions.contains(&word_address)
            || self.escaping_regions.contains(&word_address)
    }

    /// Returns whether a memory region escapes to external code.
    pub fn region_escapes(&self, address: u64) -> bool {
        let word_address = word_align(address);
        self.escaping_regions.contains(&word_address)
    }

    /// Returns the set of tainted memory regions (word-aligned addresses).
    pub fn tainted_regions(&self) -> &BTreeSet<u64> {
        &self.tainted_regions
    }

    /// Returns the set of escaping memory regions.
    pub fn escaping_regions(&self) -> &BTreeSet<u64> {
        &self.escaping_regions
    }

    /// Returns whether any escaping statement has a dynamic (non-static) offset.
    pub fn has_dynamic_escapes(&self) -> bool {
        self.has_dynamic_escapes
    }

    /// Returns the minimum start offset of any dynamic-length escape.
    pub fn min_dynamic_escape_start(&self) -> Option<u64> {
        self.min_dynamic_escape_start
    }

    /// Returns whether any memory access has a dynamic (non-static) offset.
    pub fn has_dynamic_accesses(&self) -> bool {
        self.has_dynamic_accesses
    }

    /// Returns whether any `return` statement covers the FMP slot at 0x40.
    pub fn fmp_word_escapes(&self) -> bool {
        self.fmp_word_escapes
    }

    /// Returns whether any FMP write uses a value that is not provably
    /// sbrk-bounded. See `fmp_could_be_unbounded` field doc.
    pub fn fmp_could_be_unbounded(&self) -> bool {
        self.fmp_could_be_unbounded
    }

    /// Walks the (possibly transitive) `Let` chain for `value_id` and
    /// returns true iff its source expression matches a Solidity-allocator
    /// pattern that keeps the FMP < heap_size at runtime. Recognized
    /// patterns:
    ///   - Literal in `[0x80, heap_size)` (`memoryguard(0x80)` collapses to this).
    ///     A lower pointer breaks [`Self::is_free_pointer_relative`], which
    ///     assumes `FMP >= 0x80` so that copies to `mload(0x40) + k` miss
    ///     the FMP slot. As an `add` or `and` operand, any literal below
    ///     `heap_size` is trusted as a bounded size, such as the 31 that
    ///     rounds up an allocation in a non-inlined `finalize_allocation`
    ///     whose pointer is a parameter.
    ///   - `Var(x)` where `x` itself is trusted (forwarding chain).
    ///   - `Binary { Add, ... }` where at least one operand is trusted
    ///     (the canonical `add(mload(0x40), bounded_size)` pattern, or
    ///     its variants with both operands derived from FMP arithmetic)
    ///     and no operand is a literal of at least `heap_size`, such as
    ///     `not(0xff)` (solc's form of subtracting 0x100): that addend can
    ///     only wrap the pointer below its base or move it out of the heap.
    ///     A dynamic addend stays trusted (known gap: `add(mload(0x40), calldataload(0))`).
    ///   - `Binary { And, ... }` where at least one operand is trusted
    ///     (alignment masking such as `and(add(mload(0x40), size), not(31))`
    ///     keeps a bounded value bounded).
    ///   - `MLoad { offset: 0x40, .. }` — reads the current FMP, which
    ///     sbrk-style code uses as the base for new allocations.
    ///   - `Keccak256Single` / `Keccak256Pair` — solc never writes a
    ///     hash to FMP, but the simplifier does fuse mload(0x40) +
    ///     keccak in mapping lookups; we don't actually expect this
    ///     to flow to mstore(0x40), so flagging it would be a false
    ///     negative. Out of caution we say "not trusted".
    ///
    /// Anything else (e.g. `CallDataLoad`, `SLoad`, opaque function
    /// returns, arithmetic involving untrusted values) is treated as
    /// potentially-non-bounded.
    ///
    /// Walks at most `MAX_FMP_TRUST_DEPTH` `Var` links to avoid loops.
    fn is_trusted_fmp_source(&self, value_id: u32) -> bool {
        let is_trusted_or_bounded_size =
            |operand_id: u32| match self.value_expressions.get(&operand_id) {
                Some(Expression::Literal { value, .. }) => *value < BigUint::from(self.heap_size),
                _ => self.is_trusted_fmp_source(operand_id),
            };
        let is_literal_outside_heap =
            |operand_id: u32| match self.value_expressions.get(&operand_id) {
                Some(Expression::Literal { value, .. }) => *value >= BigUint::from(self.heap_size),
                _ => false,
            };

        const MAX_FMP_TRUST_DEPTH: u32 = 32;
        let mut current = value_id;
        for _ in 0..MAX_FMP_TRUST_DEPTH {
            let Some(expression) = self.value_expressions.get(&current) else {
                return false;
            };
            match expression {
                Expression::Literal { value, .. } => {
                    return (BigUint::from(0x80u64)..BigUint::from(self.heap_size)).contains(value);
                }
                Expression::MLoad { offset, .. } => {
                    return self.extract_static_offset(offset) == Some(0x40);
                }
                Expression::Var(inner) => {
                    current = inner.0;
                    continue;
                }
                Expression::Binary {
                    operation: crate::ir::BinaryOperation::Add,
                    lhs,
                    rhs,
                } => {
                    return !is_literal_outside_heap(lhs.id.0)
                        && !is_literal_outside_heap(rhs.id.0)
                        && (is_trusted_or_bounded_size(lhs.id.0)
                            || is_trusted_or_bounded_size(rhs.id.0));
                }
                Expression::Binary {
                    operation: crate::ir::BinaryOperation::And,
                    lhs,
                    rhs,
                } => {
                    return is_trusted_or_bounded_size(lhs.id.0)
                        || is_trusted_or_bounded_size(rhs.id.0);
                }
                _ => return false,
            }
        }
        false
    }

    /// Returns the set of static offsets accessed via non-literal expressions.
    pub fn variable_accessed_offsets(&self) -> &BTreeSet<u64> {
        &self.variable_accessed_offsets
    }

    /// Returns statistics about the analysis.
    pub fn statistics(&self) -> HeapAnalysisStats {
        let total_accesses = self.memory_accesses.len();
        let aligned_accesses = self
            .memory_accesses
            .values()
            .filter(|pattern| pattern.is_aligned())
            .count();
        let static_accesses = self
            .memory_accesses
            .values()
            .filter(|pattern| pattern.is_static())
            .count();

        HeapAnalysisStats {
            total_accesses,
            aligned_accesses,
            static_accesses,
            tainted_regions: self.tainted_regions.len(),
            escaping_regions: self.escaping_regions.len(),
        }
    }
}

/// Statistics from heap analysis.
#[derive(Clone, Debug)]
pub struct HeapAnalysisStats {
    /// Total number of memory accesses analyzed.
    pub total_accesses: usize,
    /// Number of accesses that are known to be aligned.
    pub aligned_accesses: usize,
    /// Number of accesses with statically known offsets.
    pub static_accesses: usize,
    /// Number of tainted regions requiring big-endian emulation.
    pub tainted_regions: usize,
    /// Number of regions that escape to external code.
    pub escaping_regions: usize,
}

/// Results of heap analysis that can be used during code generation.
///
/// This struct captures which memory addresses can use native byte order
/// (skip byte-swapping) because they are:
/// 1. Word-aligned (offset is multiple of 32)
/// 2. Not escaping to external code
/// 3. Not tainted by unaligned writes
#[derive(Clone, Debug, Default)]
pub struct HeapOptResults {
    /// Memory addresses (word-aligned) that can use native byte order.
    /// These are addresses that are NOT in tainted_regions and NOT in escaping_regions.
    pub native_safe_regions: BTreeSet<u64>,
    /// Static offsets that are known to be safe for native access.
    pub native_safe_offsets: BTreeSet<u64>,
    /// Total number of memory accesses analyzed.
    pub total_accesses: usize,
    /// Number of accesses that have unknown/dynamic offsets.
    pub unknown_accesses: usize,
    /// Number of tainted regions (require big-endian).
    pub tainted_count: usize,
    /// Number of escaping regions (external interfaces).
    pub escaping_count: usize,
    /// Whether any escaping statement has a dynamic offset we cannot track.
    pub has_dynamic_escapes: bool,
    /// The minimum start offset of any dynamic-length escape.
    min_dynamic_escape_start: Option<u64>,
    /// Whether any memory access has a dynamic offset we cannot track.
    pub has_dynamic_accesses: bool,
    /// Whether any `return` statement covers the FMP slot at 0x40.
    /// When true, the FMP native-mode optimization is unsafe.
    fmp_word_escapes: bool,
    /// Static offsets that are accessed via non-literal (variable) expressions.
    /// These offsets may not be LLVM constants, so native mode would cause
    /// a store/load mode mismatch with literal accesses to the same offset.
    variable_accessed_offsets: BTreeSet<u64>,
    /// Whether some `mstore(0x40, value)` in the program writes a value
    /// that isn't provably sbrk-bounded. See HeapAnalysis docs.
    fmp_could_be_unbounded: bool,
    /// Whether the FMP word at 0x40 is tainted (byte/unaligned write). Forces
    /// big-endian emulation for the FMP slot; see `fmp_native_safe`.
    fmp_slot_tainted: bool,
}

impl Object {
    /// Runs heap analysis over this object and returns the native-mode results codegen consumes.
    ///
    /// Call this on the FINAL IR, after every pass that can mutate memory accesses has run — in
    /// particular `run_late_inline_loop` (late inline, outlining, and fuzzy dedup's
    /// `replace_literals_with_params`, which turns a literal memory offset into a function
    /// parameter). A literal offset proven native-LE-safe against an earlier snapshot would then be
    /// lowered byte-swapped (a variable offset can't be proven native-safe) while other still-literal
    /// accesses to the same word lower native-LE, corrupting that word's byte order. Deriving the
    /// results from the final IR keeps native-mode decisions consistent with what codegen emits.
    pub fn analyze_heap(&self, heap_size: u64) -> HeapOptResults {
        let mut analysis = HeapAnalysis::new(heap_size);
        analysis.analyze_object(self);
        HeapOptResults::from_analysis(&analysis)
    }
}

impl HeapOptResults {
    /// Creates results from a completed heap analysis.
    pub fn from_analysis(analysis: &HeapAnalysis) -> Self {
        let mut native_safe_regions = BTreeSet::new();
        let mut native_safe_offsets = BTreeSet::new();
        let mut unknown_accesses = 0;

        for (&address, pattern) in &analysis.memory_accesses {
            if matches!(pattern, AccessPattern::Unknown) {
                unknown_accesses += 1;
            } else if pattern.is_aligned() {
                let word_address = word_align(address);
                if !analysis.requires_big_endian(address) {
                    native_safe_regions.insert(word_address);
                    native_safe_offsets.insert(address);
                }
            }
        }

        HeapOptResults {
            native_safe_regions,
            native_safe_offsets,
            total_accesses: analysis.memory_accesses.len(),
            unknown_accesses,
            tainted_count: analysis.tainted_regions().len(),
            escaping_count: analysis.escaping_regions().len(),
            has_dynamic_escapes: analysis.has_dynamic_escapes(),
            min_dynamic_escape_start: analysis.min_dynamic_escape_start(),
            has_dynamic_accesses: analysis.has_dynamic_accesses(),
            fmp_word_escapes: analysis.fmp_word_escapes(),
            variable_accessed_offsets: analysis.variable_accessed_offsets().clone(),
            fmp_could_be_unbounded: analysis.fmp_could_be_unbounded(),
            fmp_slot_tainted: analysis.tainted_regions().contains(&0x40),
        }
    }

    /// Returns whether any FMP write writes a value not provably bounded
    /// by heap_size. When true, codegen optimizations that rely on
    /// `FMP < heap_size` (the post-MLoad range proof, InlineNative
    /// truncations on the FMP slot) must be skipped — those
    /// assumptions hold only for the Solidity allocator pattern.
    pub fn fmp_could_be_unbounded(&self) -> bool {
        self.fmp_could_be_unbounded
    }

    /// Checks if a static offset can use native byte order.
    pub fn can_use_native(&self, offset: u64) -> bool {
        if self.has_dynamic_accesses {
            return false;
        }
        if self.variable_accessed_offsets.contains(&offset) {
            return false;
        }
        if let Some(min_start) = self.min_dynamic_escape_start {
            let word_offset = word_align(offset);
            if word_offset >= min_start {
                return false;
            }
        }
        if self.has_dynamic_escapes && offset >= 0x60 {
            return false;
        }
        if self.native_safe_offsets.contains(&offset) {
            return true;
        }
        let word_address = word_align(offset);
        self.native_safe_regions.contains(&word_address)
    }

    /// Returns true if any optimization opportunities were found.
    pub fn has_optimizations(&self) -> bool {
        !self.native_safe_regions.is_empty()
    }

    /// Returns true if the FMP slot at 0x40 is safe for native-mode optimization.
    /// This is false when:
    /// - A static escape covers offset 0x40 (e.g., `return(0, 96)`,
    ///   `revert(0, 96)`, `log0(0, 96)`, `call(.., 0, 96, ..)`, `keccak256(0, 96)`)
    /// - A dynamic-length escape starts at or before 0x40 (e.g., `return(0, dynamic)`)
    /// - Offset 0x40 is accessed via a non-literal expression (LLVM won't see a constant)
    /// - Any memory access uses a fully dynamic offset (could touch 0x40)
    pub fn fmp_native_safe(&self) -> bool {
        if self.fmp_slot_tainted {
            return false;
        }
        if self.variable_accessed_offsets.contains(&0x40) {
            return false;
        }
        if self.has_dynamic_accesses {
            return false;
        }
        if self.fmp_word_escapes {
            return false;
        }
        if let Some(min_start) = self.min_dynamic_escape_start {
            if min_start <= 0x40 {
                return false;
            }
        }
        true
    }

    /// Returns true if ALL memory accesses can use native byte order.
    ///
    /// This is used to enable the native-only heap mode, where we only emit
    /// native heap functions and skip byte-swapping entirely. This is beneficial
    /// only when ALL accesses are native-safe; otherwise, emitting both native
    /// and non-native functions increases code size.
    ///
    /// Native-only mode is enabled when:
    /// 1. There are memory accesses that were analyzed
    /// 2. No unknown/dynamic accesses exist
    /// 3. No memory escapes to external code (calls, returns, logs)
    /// 4. No tainted regions (unaligned writes)
    pub fn all_native(&self) -> bool {
        self.total_accesses > 0
            && self.unknown_accesses == 0
            && self.tainted_count == 0
            && self.escaping_count == 0
            && !self.has_dynamic_escapes
            && !self.has_dynamic_accesses
    }

    /// Checks if ANY native optimizations are available.
    /// This is a weaker condition than `all_native()` - it means at least some
    /// accesses can use native byte order, but we may need mixed mode.
    pub fn has_any_native(&self) -> bool {
        !self.native_safe_regions.is_empty()
    }
}

/// Collects the single-binding `Let` expressions of a loop's body or `post` `statements`, including
/// those in nested `if`, `switch` and block regions, where a loop computes the values it hands on.
/// Nested loops are skipped: their bindings are out of scope for this loop's hand-overs.
fn collect_loop_definitions<'a>(
    statements: &'a [Statement],
    definitions: &mut BTreeMap<crate::ir::ValueId, &'a Expression>,
) {
    for statement in statements {
        match statement {
            Statement::Let { bindings, value } => {
                if let [binding] = bindings.as_slice() {
                    definitions.insert(*binding, value);
                }
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                collect_loop_definitions(&then_region.statements, definitions);
                if let Some(else_region) = else_region {
                    collect_loop_definitions(&else_region.statements, definitions);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    collect_loop_definitions(&case.body.statements, definitions);
                }
                if let Some(default) = default {
                    collect_loop_definitions(&default.statements, definitions);
                }
            }
            Statement::Block(region) => collect_loop_definitions(&region.statements, definitions),
            _ => {}
        }
    }
}

/// Computes the alignment of a value (highest power of 2 that divides it).
fn compute_alignment(value: u64) -> u32 {
    if value == 0 {
        return 32;
    }
    value.trailing_zeros().min(5)
}

/// Computes GCD of two numbers.
fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Records, for every output of an `if`, `switch` or `for` in `statements`, the values that can
/// flow into it, and pushes the first value of every `leave` to `leave_values`.
/// `innermost_loop` holds the post inputs and outputs of the loop that a `continue` or `break`
/// in `statements` belongs to.
fn collect_merge_inputs(
    statements: &[Statement],
    innermost_loop: Option<(&[ValueId], &[ValueId])>,
    merge_inputs: &mut BTreeMap<u32, Vec<u32>>,
    leave_values: &mut Vec<u32>,
) {
    let mut record = |merges: &[ValueId], values: &mut dyn Iterator<Item = u32>| {
        for (merge, value) in merges.iter().zip(values) {
            merge_inputs.entry(merge.0).or_default().push(value);
        }
    };
    let mut nested = Vec::new();
    for statement in statements {
        match statement {
            Statement::If {
                inputs,
                then_region,
                else_region,
                outputs,
                ..
            } => {
                record(
                    outputs,
                    &mut then_region.yields.iter().map(|value| value.id.0),
                );
                let else_values = else_region.as_ref().map_or(inputs, |region| &region.yields);
                record(outputs, &mut else_values.iter().map(|value| value.id.0));
                nested.push((&then_region.statements, innermost_loop));
                if let Some(region) = else_region {
                    nested.push((&region.statements, innermost_loop));
                }
            }
            Statement::Switch {
                inputs,
                cases,
                default,
                outputs,
                ..
            } => {
                for case in cases {
                    record(
                        outputs,
                        &mut case.body.yields.iter().map(|value| value.id.0),
                    );
                    nested.push((&case.body.statements, innermost_loop));
                }
                let default_values = default.as_ref().map_or(inputs, |region| &region.yields);
                record(outputs, &mut default_values.iter().map(|value| value.id.0));
                if let Some(region) = default {
                    nested.push((&region.statements, innermost_loop));
                }
            }
            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                body,
                post_input_variables,
                post,
                outputs,
                ..
            } => {
                record(
                    loop_variables,
                    &mut initial_values.iter().map(|value| value.id.0),
                );
                record(
                    loop_variables,
                    &mut post.yields.iter().map(|value| value.id.0),
                );
                record(
                    post_input_variables,
                    &mut body.yields.iter().map(|value| value.id.0),
                );
                record(
                    outputs,
                    &mut loop_variables.iter().map(|variable| variable.0),
                );
                let this_loop = Some((post_input_variables.as_slice(), outputs.as_slice()));
                nested.push((condition_statements, this_loop));
                nested.push((&body.statements, this_loop));
                nested.push((&post.statements, this_loop));
            }
            Statement::Block(region) => nested.push((&region.statements, innermost_loop)),
            Statement::Continue { values } => {
                if let Some((post_inputs, _)) = innermost_loop {
                    record(post_inputs, &mut values.iter().map(|value| value.id.0));
                }
            }
            Statement::Break { values } => {
                if let Some((_, loop_outputs)) = innermost_loop {
                    record(loop_outputs, &mut values.iter().map(|value| value.id.0));
                }
            }
            Statement::Leave { return_values } => {
                leave_values.extend(return_values.first().map(|value| value.id.0));
            }
            _ => {}
        }
    }
    for (statements, loop_scope) in nested {
        collect_merge_inputs(statements, loop_scope, merge_inputs, leave_values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    /// The default `--heap-size`.
    const TEST_HEAP_SIZE: u64 = 131_072;

    /// Builds a `Statement::Let` binding `id` to the literal `value`.
    /// Values wider than 64 bits need `big_literal_binding`.
    fn literal_binding(id: u32, value: u64) -> Statement {
        big_literal_binding(id, BigUint::from(value))
    }

    /// Builds a `Statement::Let` binding `id` to the literal `value`, which may be wider than
    /// 64 bits, such as 2^128 or `not(0x7f)`.
    fn big_literal_binding(id: u32, value: BigUint) -> Statement {
        use crate::ir::{BitWidth, Type, ValueId};
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Literal {
                value,
                value_type: Type::Int(BitWidth::I256),
            },
        }
    }

    #[test]
    fn test_alignment_computation() {
        assert_eq!(compute_alignment(0), 32);
        assert_eq!(compute_alignment(1), 0);
        assert_eq!(compute_alignment(2), 1);
        assert_eq!(compute_alignment(4), 2);
        assert_eq!(compute_alignment(32), 5);
        assert_eq!(compute_alignment(64), 5);
        assert_eq!(compute_alignment(33), 0);
    }

    #[test]
    fn test_access_pattern_classification() {
        assert!(AccessPattern::AlignedStatic(0).is_aligned());
        assert!(AccessPattern::AlignedStatic(32).is_aligned());
        assert!(AccessPattern::AlignedDynamic.is_aligned());
        assert!(!AccessPattern::UnalignedStatic(1).is_aligned());
        assert!(!AccessPattern::Unknown.is_aligned());

        assert!(AccessPattern::AlignedStatic(0).is_static());
        assert!(AccessPattern::UnalignedStatic(1).is_static());
        assert!(!AccessPattern::AlignedDynamic.is_static());
        assert!(!AccessPattern::Unknown.is_static());
    }

    #[test]
    fn test_gcd() {
        assert_eq!(gcd(32, 64), 32);
        assert_eq!(gcd(12, 18), 6);
        assert_eq!(gcd(17, 13), 1);
        assert_eq!(gcd(0, 5), 5);
    }

    /// Builds the statements that establish the FMP from a trusted literal (`mstore(0x40, 0x80)`,
    /// which sets no direct unboundedness flag) using value IDs 0 (`0x80`) and 1 (`0x40`).
    fn establish_fmp_statements() -> Vec<Statement> {
        use crate::ir::ValueId;
        vec![
            literal_binding(0, 0x80),
            literal_binding(1, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(1)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::from_address(&BigUint::from(0x40u64)),
            },
        ]
    }

    /// Builds the statements for an unaligned `mstore(0x38, huge)` overlapping the FMP word
    /// `[0x40, 0x60)`, using value IDs `first_id` (the huge value) and `first_id + 1` (`0x38`).
    fn overlap_store_statements(first_id: u32) -> Vec<Statement> {
        use crate::ir::ValueId;
        vec![
            literal_binding(first_id, 0xffff_ffff_ffff_ffff),
            literal_binding(first_id + 1, 0x38),
            Statement::MStore {
                offset: Value::int(ValueId(first_id + 1)),
                value: Value::int(ValueId(first_id)),
                region: MemoryRegion::from_address(&BigUint::from(0x38u64)),
            },
        ]
    }

    /// Builds the statements for an `mload(0x40)` FMP observation, using value IDs `first_id`
    /// (`0x40`) and `first_id + 1` (the loaded value).
    fn observe_fmp_statements(first_id: u32) -> Vec<Statement> {
        use crate::ir::ValueId;
        vec![
            literal_binding(first_id, 0x40),
            Statement::Let {
                bindings: vec![ValueId(first_id + 1)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(first_id)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
        ]
    }

    /// Wraps runtime-code `statements` (and optional `functions`) into an object.
    fn object_with_code(statements: Vec<Statement>, functions: Vec<crate::ir::Function>) -> Object {
        use crate::ir::Block;
        Object {
            name: "T".to_string(),
            code: Block { statements },
            functions: functions
                .into_iter()
                .map(|function| (function.id, function))
                .collect(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        }
    }

    /// Builds an object whose runtime code binds `dest` (id 10) and copies 23
    /// bytes of calldata to it, then reads `mload(0x40)`.
    fn object_with_dynamic_copy(dest_setup: Vec<Statement>) -> Object {
        use crate::ir::{Block, ValueId};
        let mut statements = dest_setup;
        statements.push(literal_binding(20, 0));
        statements.push(literal_binding(21, 23));
        statements.push(Statement::CallDataCopy {
            destination: Value::int(ValueId(10)),
            offset: Value::int(ValueId(20)),
            length: Value::int(ValueId(21)),
        });
        Object {
            name: "T".to_string(),
            code: Block { statements },
            functions: std::collections::BTreeMap::new(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        }
    }

    /// Builds an object whose runtime code establishes the FMP, does an unaligned
    /// `mstore(0x38, huge)` overlapping the FMP word, then runs `tail`.
    fn object_with_overlap_store(tail: Vec<Statement>) -> Object {
        let mut statements = establish_fmp_statements();
        statements.extend(overlap_store_statements(2));
        statements.extend(tail);
        object_with_code(statements, vec![])
    }

    /// An overlap store observed by a later `mload(0x40)` must flag the FMP as
    /// possibly unbounded so codegen skips the `FMP < heap_size` range proof.
    #[test]
    fn overlap_store_observed_by_fmp_load_flags_unbounded() {
        let results =
            object_with_overlap_store(observe_fmp_statements(4)).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "an mstore overlapping 0x40 read back via mload(0x40) is observed corruption"
        );
    }

    /// An overlap store whose only successor is a frame terminator (the solc
    /// revert/return ABI-encoding pattern) discards the corrupted FMP unobserved,
    /// so it must NOT flag unboundedness — otherwise the FMP optimization is lost
    /// for nearly every contract with a `require`-with-message.
    #[test]
    fn overlap_store_then_revert_stays_bounded() {
        use crate::ir::ValueId;
        let tail = vec![Statement::Revert {
            offset: Value::int(ValueId(1)),
            length: Value::int(ValueId(0)),
        }];
        let results = object_with_overlap_store(tail).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "an mstore overlapping 0x40 followed only by a revert is unobserved corruption"
        );
    }

    /// Builds a function (ID 0, no parameters or returns) whose body is an unaligned
    /// `mstore(0x38, huge)` overlapping the FMP word, exiting by fall-through.
    fn function_with_overlap_store() -> crate::ir::Function {
        use crate::ir::{Block, Function, FunctionId};
        let mut function = Function::new(FunctionId(0), "corrupt_free_pointer".to_string());
        function.body = Block {
            statements: overlap_store_statements(10),
        };
        function
    }

    /// Builds an object whose runtime code establishes the FMP, calls the
    /// FMP-corrupting function, then runs `tail`.
    fn object_with_corrupting_call(tail: Vec<Statement>) -> Object {
        use crate::ir::FunctionId;
        let mut statements = establish_fmp_statements();
        statements.push(Statement::Expression(Expression::Call {
            function: FunctionId(0),
            arguments: vec![],
        }));
        statements.extend(tail);
        object_with_code(statements, vec![function_with_overlap_store()])
    }

    /// A callee that exits with the FMP word corrupted (overlap store, then
    /// fall-through return) propagates the corruption to its caller: a caller-side
    /// `mload(0x40)` after the call is observed corruption and must flag.
    #[test]
    fn callee_overlap_store_observed_by_caller_flags_unbounded() {
        let results =
            object_with_corrupting_call(observe_fmp_statements(4)).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "corruption escaping a callee and read back via mload(0x40) is observed"
        );
    }

    /// A callee-corrupted FMP that the caller discards with an immediate frame
    /// terminator is never observed, so it must NOT flag unboundedness.
    #[test]
    fn callee_overlap_store_then_caller_revert_stays_bounded() {
        use crate::ir::ValueId;
        let tail = vec![Statement::Revert {
            offset: Value::int(ValueId(1)),
            length: Value::int(ValueId(0)),
        }];
        let results = object_with_corrupting_call(tail).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "callee corruption discarded by an immediate revert is unobserved"
        );
    }

    /// A `break` right after an overlap store exits the loop *past* a `post` that
    /// re-establishes the FMP, so an `mload(0x40)` after the loop still reads the
    /// corrupted pointer and must flag: the restoring `post` runs only on the
    /// `continue`/fall-through path and must not mask the `break` path.
    #[test]
    fn break_skipping_post_fmp_restore_flags_unbounded() {
        use crate::ir::{Region, Type, ValueId};
        let mut body_statements = overlap_store_statements(10);
        body_statements.push(Statement::Break { values: vec![] });
        let post_statements = vec![
            literal_binding(20, 0x80),
            literal_binding(21, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(21)),
                value: Value::int(ValueId(20)),
                region: MemoryRegion::from_address(&BigUint::from(0x40u64)),
            },
        ];
        let mut statements = establish_fmp_statements();
        statements.push(Statement::For {
            initial_values: vec![],
            loop_variables: vec![],
            condition_statements: vec![],
            condition: Expression::Literal {
                value: BigUint::from(1u64),
                value_type: Type::default(),
            },
            body: Region {
                statements: body_statements,
                yields: vec![],
            },
            post_input_variables: vec![],
            post: Region {
                statements: post_statements,
                yields: vec![],
            },
            outputs: vec![],
        });
        statements.extend(observe_fmp_statements(30));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a break skips the loop post, so its FMP restore must not mask the corrupted break path"
        );
    }

    /// Builds a `For` loop with the given `condition` expression and empty
    /// condition-statements, body and post.
    fn loop_with_condition(condition: Expression) -> Statement {
        use crate::ir::Region;
        Statement::For {
            initial_values: vec![],
            loop_variables: vec![],
            condition_statements: vec![],
            condition,
            body: Region {
                statements: vec![],
                yields: vec![],
            },
            post_input_variables: vec![],
            post: Region {
                statements: vec![],
                yields: vec![],
            },
            outputs: vec![],
        }
    }

    /// A loop *condition* that reads `mload(0x40)` while the FMP word is corrupted by a
    /// preceding overlap store observes the corrupted pointer: the condition is evaluated
    /// every iteration, so [`HeapAnalysis::scan_fmp_corruption`] must scan it and flag
    /// unboundedness. With an empty body/post and no post-loop load, scanning the condition
    /// is the only thing that sees the corruption.
    #[test]
    fn condition_observes_overlap_store_flags_unbounded() {
        use crate::ir::ValueId;

        let mut statements = establish_fmp_statements();
        statements.extend(overlap_store_statements(2));
        statements.push(literal_binding(4, 0x40));
        statements.push(loop_with_condition(Expression::MLoad {
            offset: Value::int(ValueId(4)),
            region: MemoryRegion::Unknown,
        }));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a loop condition loading from static offset 0x40 after an overlap store observes the corruption"
        );

        let mut statements = establish_fmp_statements();
        statements.extend(overlap_store_statements(2));
        statements.push(loop_with_condition(Expression::MLoad {
            offset: Value::int(ValueId(4)),
            region: MemoryRegion::FreePointerSlot,
        }));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a loop condition loading from the free pointer slot after an overlap store observes the corruption"
        );
    }

    /// The benign counterpart: a loop condition reads `mload(0x40)` but no overlap store
    /// corrupts the pointer, so the observation must NOT flag unboundedness — guards the
    /// condition scan against over-flagging.
    #[test]
    fn condition_reads_uncorrupted_fmp_stays_bounded() {
        use crate::ir::ValueId;
        let mut statements = establish_fmp_statements();
        statements.push(literal_binding(4, 0x40));
        statements.push(loop_with_condition(Expression::MLoad {
            offset: Value::int(ValueId(4)),
            region: MemoryRegion::FreePointerSlot,
        }));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "reading mload(0x40) in a loop condition without corruption is not observed corruption"
        );
    }

    /// A dynamic copy destination that is not provably free-pointer-relative
    /// (`and(calldataload(p), 0xff)`, range `[0, 0xff]`) can land on the FMP word,
    /// so it must flag the FMP as possibly unbounded.
    #[test]
    fn dynamic_copy_non_fmp_dest_flags_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let dest_setup = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::CallDataLoad {
                    offset: Value::int(ValueId(0)),
                },
            },
            literal_binding(2, 0xff),
            Statement::Let {
                bindings: vec![ValueId(10)],
                value: Expression::Binary {
                    operation: BinaryOperation::And,
                    lhs: Value::int(ValueId(1)),
                    rhs: Value::int(ValueId(2)),
                },
            },
        ];
        let results = object_with_dynamic_copy(dest_setup).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a masked calldata destination can hit the FMP word"
        );
    }

    /// A dynamic copy destination that is free-pointer-relative
    /// (`add(mload(0x40), 0x20)`, `>= 0x80`) cannot hit the FMP word, so it must
    /// not disable the FMP optimization.
    #[test]
    fn dynamic_copy_fmp_relative_dest_stays_bounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let dest_setup = vec![
            literal_binding(0, 0x40),
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(0)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
            literal_binding(2, 0x20),
            Statement::Let {
                bindings: vec![ValueId(10)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(ValueId(1)),
                    rhs: Value::int(ValueId(2)),
                },
            },
        ];
        let results = object_with_dynamic_copy(dest_setup).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "an add(mload(0x40), k) destination is >= 0x80 and cannot hit the FMP word"
        );
    }

    fn object_with_mcopy(setup: Vec<Statement>, length: u64) -> Object {
        use crate::ir::ValueId;
        let mut statements = setup;
        statements.push(literal_binding(11, 0x80));
        statements.push(literal_binding(12, length));
        statements.push(Statement::MCopy {
            destination: Value::int(ValueId(10)),
            source: Value::int(ValueId(11)),
            length: Value::int(ValueId(12)),
        });
        object_with_code(statements, vec![])
    }

    fn object_with_external_call(setup: Vec<Statement>, return_length: u64) -> Object {
        use crate::ir::{CallKind, ValueId};
        let mut statements = setup;
        statements.push(literal_binding(11, return_length));
        statements.push(literal_binding(12, 0));
        statements.push(Statement::ExternalCall {
            kind: CallKind::StaticCall,
            gas: Value::int(ValueId(12)),
            address: Value::int(ValueId(12)),
            value: None,
            args_offset: Value::int(ValueId(12)),
            args_length: Value::int(ValueId(12)),
            ret_offset: Value::int(ValueId(10)),
            ret_length: Value::int(ValueId(11)),
            result: ValueId(13),
        });
        object_with_code(statements, vec![])
    }

    #[test]
    fn mcopy_onto_fmp_word_flags_unbounded() {
        let results =
            object_with_mcopy(vec![literal_binding(10, 0x40)], 0x20).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    #[test]
    fn mcopy_dynamic_destination_flags_unbounded() {
        use crate::ir::ValueId;
        let setup = vec![Statement::Let {
            bindings: vec![ValueId(10)],
            value: Expression::CallDataLoad {
                offset: Value::int(ValueId(0)),
            },
        }];
        let results = object_with_mcopy(setup, 0x20).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    #[test]
    fn external_call_return_onto_fmp_word_flags_unbounded() {
        let results = object_with_external_call(vec![literal_binding(10, 0x40)], 0x20)
            .analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    #[test]
    fn external_call_zero_length_return_stays_bounded() {
        use crate::ir::ValueId;
        let setup = vec![Statement::Let {
            bindings: vec![ValueId(10)],
            value: Expression::CallDataLoad {
                offset: Value::int(ValueId(0)),
            },
        }];
        let results = object_with_external_call(setup, 0).analyze_heap(TEST_HEAP_SIZE);
        assert!(!results.fmp_could_be_unbounded());
    }

    /// Binds `id` to `calldataload(0)`, a value the analysis cannot bound.
    fn calldata_binding(id: u32) -> Statement {
        use crate::ir::ValueId;
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::CallDataLoad {
                offset: Value::int(ValueId(0)),
            },
        }
    }

    /// Binds `id` to `mload(0x40)`, using `id + 1` for the offset.
    fn free_pointer_binding(id: u32) -> Vec<Statement> {
        use crate::ir::ValueId;
        vec![
            literal_binding(id + 1, 0x40),
            Statement::Let {
                bindings: vec![ValueId(id)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(id + 1)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
        ]
    }

    /// Builds `mcopy(destination, 0x80, 0x20)` using value IDs `first_id` and `first_id + 1`.
    fn mcopy_word_to(destination: u32, first_id: u32) -> Vec<Statement> {
        use crate::ir::ValueId;
        vec![
            literal_binding(first_id, 0x80),
            literal_binding(first_id + 1, 0x20),
            Statement::MCopy {
                destination: Value::int(ValueId(destination)),
                source: Value::int(ValueId(first_id)),
                length: Value::int(ValueId(first_id + 1)),
            },
        ]
    }

    /// Builds an object with a function `copy_word(p)` that copies a word to `add(p, 0x20)`,
    /// called once per value ID in `arguments` after `setup`.
    fn object_with_copying_function(setup: Vec<Statement>, arguments: &[u32]) -> Object {
        use crate::ir::{BinaryOperation, Function, FunctionId, Type, ValueId};
        let mut function = Function::new(FunctionId(0), "copy_word".to_string());
        function.parameters = vec![(ValueId(30), Type::default())];
        let mut body = vec![
            literal_binding(31, 0x20),
            Statement::Let {
                bindings: vec![ValueId(32)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(ValueId(30)),
                    rhs: Value::int(ValueId(31)),
                },
            },
        ];
        body.extend(mcopy_word_to(32, 33));
        function.body = Block { statements: body };
        let mut statements = setup;
        for argument in arguments {
            statements.push(Statement::Expression(Expression::Call {
                function: FunctionId(0),
                arguments: vec![Value::int(ValueId(*argument))],
            }));
        }
        object_with_code(statements, vec![function])
    }

    #[test]
    fn mcopy_static_start_dynamic_length_below_fmp_word_flags_unbounded() {
        use crate::ir::ValueId;
        let statements = vec![
            literal_binding(10, 0),
            literal_binding(11, 0x80),
            calldata_binding(12),
            Statement::MCopy {
                destination: Value::int(ValueId(10)),
                source: Value::int(ValueId(11)),
                length: Value::int(ValueId(12)),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    #[test]
    fn external_call_static_return_start_dynamic_length_flags_unbounded() {
        use crate::ir::{CallKind, ValueId};
        let statements = vec![
            literal_binding(10, 0),
            calldata_binding(11),
            Statement::ExternalCall {
                kind: CallKind::StaticCall,
                gas: Value::int(ValueId(10)),
                address: Value::int(ValueId(10)),
                value: None,
                args_offset: Value::int(ValueId(10)),
                args_length: Value::int(ValueId(10)),
                ret_offset: Value::int(ValueId(10)),
                ret_length: Value::int(ValueId(11)),
                result: ValueId(12),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    #[test]
    fn mcopy_through_free_pointer_parameter_stays_bounded() {
        let results = object_with_copying_function(free_pointer_binding(1), &[1])
            .analyze_heap(TEST_HEAP_SIZE);
        assert!(!results.fmp_could_be_unbounded());
    }

    #[test]
    fn mcopy_through_parameter_with_untrusted_call_site_flags_unbounded() {
        let mut setup = free_pointer_binding(1);
        setup.push(calldata_binding(3));
        let results = object_with_copying_function(setup, &[1, 3]).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    /// Builds an object that copies a word to the result of `pick(mload(0x40))`, which returns
    /// its parameter, or `calldataload(0)` through `leave` when `leave_untrusted` is set.
    fn object_with_returned_destination(leave_untrusted: bool) -> Object {
        use crate::ir::{Function, FunctionId, Region, Type, ValueId};
        let mut function = Function::new(FunctionId(0), "pick".to_string());
        function.parameters = vec![(ValueId(40), Type::default())];
        function.returns = vec![Type::default()];
        let mut body = Vec::new();
        if leave_untrusted {
            body.push(calldata_binding(41));
            body.push(Statement::If {
                condition: Value::int(ValueId(41)),
                inputs: vec![],
                then_region: Region {
                    statements: vec![Statement::Leave {
                        return_values: vec![Value::int(ValueId(41))],
                    }],
                    yields: vec![],
                },
                else_region: None,
                outputs: vec![],
            });
        }
        body.push(Statement::Let {
            bindings: vec![ValueId(42)],
            value: Expression::Var(ValueId(40)),
        });
        function.body = Block { statements: body };
        function.return_values = vec![ValueId(42)];

        let mut statements = free_pointer_binding(1);
        statements.push(Statement::Let {
            bindings: vec![ValueId(10)],
            value: Expression::Call {
                function: FunctionId(0),
                arguments: vec![Value::int(ValueId(1))],
            },
        });
        statements.extend(mcopy_word_to(10, 11));
        object_with_code(statements, vec![function])
    }

    #[test]
    fn mcopy_to_returned_free_pointer_stays_bounded() {
        let results = object_with_returned_destination(false).analyze_heap(TEST_HEAP_SIZE);
        assert!(!results.fmp_could_be_unbounded());
    }

    #[test]
    fn mcopy_to_untrusted_leave_value_flags_unbounded() {
        let results = object_with_returned_destination(true).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    /// Builds an object whose loop copies a word to a pointer that starts at `mload(0x40)` and
    /// advances by `0x20`, or is replaced by `calldataload(0)` when `post_untrusted` is set.
    fn object_with_loop_carried_destination(post_untrusted: bool) -> Object {
        use crate::ir::{BinaryOperation, Region, ValueId};
        let post_statement = if post_untrusted {
            calldata_binding(64)
        } else {
            Statement::Let {
                bindings: vec![ValueId(64)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(ValueId(63)),
                    rhs: Value::int(ValueId(62)),
                },
            }
        };
        let mut statements = free_pointer_binding(1);
        statements.push(Statement::For {
            initial_values: vec![Value::int(ValueId(1))],
            loop_variables: vec![ValueId(60)],
            condition_statements: vec![],
            condition: Expression::CallDataLoad {
                offset: Value::int(ValueId(0)),
            },
            body: Region {
                statements: mcopy_word_to(60, 61),
                yields: vec![Value::int(ValueId(60))],
            },
            post_input_variables: vec![ValueId(63)],
            post: Region {
                statements: vec![post_statement],
                yields: vec![Value::int(ValueId(64))],
            },
            outputs: vec![ValueId(65)],
        });
        object_with_code(statements, vec![])
    }

    #[test]
    fn mcopy_to_loop_carried_free_pointer_stays_bounded() {
        let results = object_with_loop_carried_destination(false).analyze_heap(TEST_HEAP_SIZE);
        assert!(!results.fmp_could_be_unbounded());
    }

    #[test]
    fn mcopy_to_loop_carried_untrusted_pointer_flags_unbounded() {
        let results = object_with_loop_carried_destination(true).analyze_heap(TEST_HEAP_SIZE);
        assert!(results.fmp_could_be_unbounded());
    }

    /// Builds an object that stores the literal `pointer` to the FMP slot and reads it back.
    fn object_with_fmp_literal_store(pointer: u64) -> Object {
        use crate::ir::ValueId;
        let mut statements = vec![
            literal_binding(0, pointer),
            literal_binding(1, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(1)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::FreePointerSlot,
            },
        ];
        statements.extend(observe_fmp_statements(2));
        object_with_code(statements, vec![])
    }

    #[test]
    fn fmp_literal_below_heap_size_is_trusted() {
        let results = object_with_fmp_literal_store(0x80).analyze_heap(TEST_HEAP_SIZE);
        assert!(!results.fmp_could_be_unbounded());
    }

    #[test]
    fn fmp_literal_at_or_above_heap_size_is_untrusted() {
        for pointer in [TEST_HEAP_SIZE, 0xdeadbeef, u64::MAX] {
            let results = object_with_fmp_literal_store(pointer).analyze_heap(TEST_HEAP_SIZE);
            assert!(
                results.fmp_could_be_unbounded(),
                "a literal FMP of {pointer:#x} must not be trusted"
            );
        }
    }

    #[test]
    fn loop_carried_offsets_are_dynamic() {
        use crate::ir::{BinaryOperation, Region, Type, ValueId};
        let statements = vec![
            literal_binding(0, 0x80),
            literal_binding(2, 0x1234),
            literal_binding(3, 0x20),
            Statement::For {
                initial_values: vec![Value::int(ValueId(0))],
                loop_variables: vec![ValueId(1)],
                condition_statements: vec![],
                condition: Expression::Literal {
                    value: BigUint::from(1u64),
                    value_type: Type::default(),
                },
                body: Region {
                    statements: vec![Statement::MStore {
                        offset: Value::int(ValueId(1)),
                        value: Value::int(ValueId(2)),
                        region: MemoryRegion::Unknown,
                    }],
                    yields: vec![Value::int(ValueId(1))],
                },
                post_input_variables: vec![ValueId(4)],
                post: Region {
                    statements: vec![Statement::Let {
                        bindings: vec![ValueId(5)],
                        value: Expression::Binary {
                            operation: BinaryOperation::Add,
                            lhs: Value::int(ValueId(4)),
                            rhs: Value::int(ValueId(3)),
                        },
                    }],
                    yields: vec![Value::int(ValueId(5))],
                },
                outputs: vec![ValueId(6)],
            },
            literal_binding(7, 0xa0),
            Statement::Let {
                bindings: vec![ValueId(8)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(7)),
                    region: MemoryRegion::Dynamic,
                },
            },
            Statement::MStore {
                offset: Value::int(ValueId(6)),
                value: Value::int(ValueId(2)),
                region: MemoryRegion::Unknown,
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.has_dynamic_accesses,
            "the loop store and the post-loop store have no static offset"
        );
        assert!(
            !results.native_safe_offsets.contains(&0x80),
            "the first iteration's address must not be recorded as the loop's static access"
        );
        assert!(
            !results.can_use_native(0xa0),
            "a word the loop writes byte-swapped must not be read native"
        );
    }

    /// Builds `for { let p := seed } 1 { p := operation(p, step) } { mstore(p, 0x1234) }` with ids
    /// 0 to 6 (the seed, `p`, `0x1234`, `step`, `post`'s input, its result and the loop output),
    /// binding `step` inside `post` as the translator does.
    fn counter_store_loop(
        seed: u64,
        operation: crate::ir::BinaryOperation,
        step: BigUint,
    ) -> Vec<Statement> {
        use crate::ir::{Region, Type, ValueId};
        vec![
            literal_binding(0, seed),
            literal_binding(2, 0x1234),
            Statement::For {
                initial_values: vec![Value::int(ValueId(0))],
                loop_variables: vec![ValueId(1)],
                condition_statements: vec![],
                condition: Expression::Literal {
                    value: BigUint::from(1u64),
                    value_type: Type::default(),
                },
                body: Region {
                    statements: vec![word_store(1)],
                    yields: vec![Value::int(ValueId(1))],
                },
                post_input_variables: vec![ValueId(4)],
                post: Region {
                    statements: vec![
                        Statement::Let {
                            bindings: vec![ValueId(3)],
                            value: Expression::Literal {
                                value: step,
                                value_type: Type::default(),
                            },
                        },
                        binary_binding(5, operation, 4, 3),
                    ],
                    yields: vec![Value::int(ValueId(5))],
                },
                outputs: vec![ValueId(6)],
            },
        ]
    }

    /// Builds `mstore(offset, 0x1234)`, storing the value `counter_store_loop` binds to id 2.
    fn word_store(offset: u32) -> Statement {
        use crate::ir::ValueId;
        Statement::MStore {
            offset: Value::int(ValueId(offset)),
            value: Value::int(ValueId(2)),
            region: MemoryRegion::Unknown,
        }
    }

    /// Builds `let id := operation(lhs, rhs)`.
    fn binary_binding(
        id: u32,
        operation: crate::ir::BinaryOperation,
        lhs: u32,
        rhs: u32,
    ) -> Statement {
        use crate::ir::ValueId;
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Binary {
                operation,
                lhs: Value::int(ValueId(lhs)),
                rhs: Value::int(ValueId(rhs)),
            },
        }
    }

    /// `not(0x1f)`, since solc's optimizer rewrites `sub(p, 0x20)` into `add(p, not(0x1f))`.
    fn not_0x1f() -> BigUint {
        (BigUint::from(1u32) << revive_common::BIT_LENGTH_WORD) - BigUint::from(0x20u32)
    }

    /// Builds a loop nested in another loop's body whose own body is `statements`, reusing the
    /// outer seed (id 0) and ids 20 to 22.
    fn nested_loop(statements: Vec<Statement>) -> Statement {
        use crate::ir::{Region, Type, ValueId};
        Statement::For {
            initial_values: vec![Value::int(ValueId(0))],
            loop_variables: vec![ValueId(20)],
            condition_statements: vec![],
            condition: Expression::Literal {
                value: BigUint::from(1u64),
                value_type: Type::default(),
            },
            body: Region {
                statements,
                yields: vec![Value::int(ValueId(20))],
            },
            post_input_variables: vec![ValueId(21)],
            post: Region {
                statements: vec![],
                yields: vec![Value::int(ValueId(21))],
            },
            outputs: vec![ValueId(22)],
        }
    }

    /// A counter from `0x40` stores on the FMP word in its first iteration, so a later
    /// `mload(0x40)` must not be truncated by the range proof.
    #[test]
    fn ascending_counter_store_from_fmp_word_is_unbounded() {
        use crate::ir::BinaryOperation;
        let statements = counter_store_loop(0x40, BinaryOperation::Add, BigUint::from(0x20u32));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the first store lands on the FMP word"
        );
    }

    /// A counter from `0x5f`, the last start whose word store overlaps the FMP word, overwrites
    /// its last byte, which pins the `0x60` threshold from below.
    #[test]
    fn ascending_counter_store_overlapping_fmp_word_is_unbounded() {
        use crate::ir::BinaryOperation;
        let statements = counter_store_loop(0x5f, BinaryOperation::Add, BigUint::from(0x20u32));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a store at 0x5f overlaps the last byte of the FMP word"
        );
    }

    /// `sub(p, 0x20)` from `0x80` stores on the FMP word in its third iteration, so a counter
    /// that is not handed on stepped up has no minimum.
    #[test]
    fn descending_counter_store_is_unbounded() {
        use crate::ir::BinaryOperation;
        let statements = counter_store_loop(0x80, BinaryOperation::Sub, BigUint::from(0x20u32));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "sub(p, 0x20) from 0x80 reaches the FMP word"
        );
    }

    /// A counter from `0x60` that only steps up never reaches the FMP word, so the range proof
    /// stays; this pins the `0x60` threshold from above.
    #[test]
    fn ascending_counter_store_above_fmp_word_stays_bounded() {
        use crate::ir::BinaryOperation;
        let statements = counter_store_loop(0x60, BinaryOperation::Add, BigUint::from(0x20u32));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "stores from 0x60 upward cannot reach the FMP word"
        );
    }

    /// The translator binds a step inside `post`, but other passes can leave it bound before the
    /// loop, where the analysis has already resolved it; the counter still only steps up.
    #[test]
    fn step_bound_before_loop_keeps_counter_bounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0x60, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { post, .. }) = statements.last_mut() else {
            unreachable!()
        };
        let step = post.statements.remove(0);
        statements.insert(0, step);
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "a static step bound before the loop steps the counter up"
        );
    }

    /// solc's optimizer rewrites `sub(p, 0x20)` into `add(p, not(0x1f))`, which descends onto the
    /// FMP word: only a static step makes an `add` step up.
    #[test]
    fn descending_by_addition_counter_store_is_unbounded() {
        use crate::ir::BinaryOperation;
        let statements = counter_store_loop(0x80, BinaryOperation::Add, not_0x1f());
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "add(p, not(0x1f)) from 0x80 reaches the FMP word"
        );
    }

    /// The same descending counter as a copy destination overwrites the FMP word too, so its seed
    /// `0x80` must not exempt the copy.
    #[test]
    fn descending_by_addition_counter_copy_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, not_0x1f());
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0),
            literal_binding(8, 0x20),
            Statement::CallDataCopy {
                destination: Value::int(ValueId(1)),
                offset: Value::int(ValueId(7)),
                length: Value::int(ValueId(8)),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a copy to add(p, not(0x1f)) from 0x80 covers the FMP word"
        );
    }

    /// solc's array copy loop steps its write pointer up in the body (`mpos := add(mpos, 32)`)
    /// and leaves `post` alone. Stepped up by `add(0x20, p)`, the other operand order, a pointer
    /// from `0xa0` never reaches the FMP word, so the range proof stays.
    #[test]
    fn ascending_in_body_counter_store_above_fmp_word_stays_bounded() {
        use crate::ir::{BinaryOperation, Region, ValueId};
        let mut statements = counter_store_loop(0xa0, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, post, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.extend([
            literal_binding(7, 0x20),
            binary_binding(8, BinaryOperation::Add, 7, 1),
        ]);
        body.yields = vec![Value::int(ValueId(8))];
        *post = Region {
            statements: vec![],
            yields: vec![Value::int(ValueId(4))],
        };
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "a pointer ascending by 0x20 from 0xa0 stays above the FMP word"
        );
    }

    /// A body that hands `post` a new value (`0x20`, which `post` steps onto `0x40`) instead of
    /// the counter breaks the ascent.
    #[test]
    fn body_with_other_counter_value_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.push(literal_binding(7, 0x20));
        body.yields = vec![Value::int(ValueId(7))];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the body's 0x20 steps onto the FMP word"
        );
    }

    /// A `continue` hands `post` its values like the body's yield, so one that hands on a new
    /// value (`0x20`) breaks the ascent too.
    #[test]
    fn continue_with_other_counter_value_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.extend([
            literal_binding(7, 0x20),
            Statement::Continue {
                values: vec![Value::int(ValueId(7))],
            },
        ]);
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the continue's 0x20 steps onto the FMP word"
        );
    }

    /// `add(0x80, k)` with `k` descending from `0` wraps below `0x80` onto the FMP word, so a
    /// counter that does not step up must carry no minimum, not `0`.
    #[test]
    fn literal_plus_descending_counter_store_is_unbounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0, BinaryOperation::Sub, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0x80),
            binary_binding(8, BinaryOperation::Add, 7, 1),
            word_store(8),
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "add(0x80, k) with k descending from 0 reaches the FMP word"
        );
    }

    /// The same `add(0x80, k)` as a copy destination: `is_free_pointer_relative` accepts its
    /// literal base, but a destination computed from a counter is decided by its range.
    #[test]
    fn literal_plus_descending_counter_copy_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0, BinaryOperation::Sub, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0x80),
            binary_binding(8, BinaryOperation::Add, 7, 1),
            literal_binding(9, 0),
            literal_binding(10, 0x20),
            Statement::CallDataCopy {
                destination: Value::int(ValueId(8)),
                offset: Value::int(ValueId(9)),
                length: Value::int(ValueId(10)),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a copy to add(0x80, k) with k descending from 0 covers the FMP word"
        );
    }

    /// Builds `counter_store_loop` stepping by 1 whose body stores at `add(base, shl(5, p))`, or
    /// at `shl(5, p)` without a base: newyork's rewrite of `mul(p, 0x20)`, with the shift in `lhs`.
    fn shifted_counter_store_loop(seed: u64, base: Option<u64>) -> Vec<Statement> {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(seed, BinaryOperation::Add, BigUint::from(1u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 5),
            binary_binding(8, BinaryOperation::Shl, 7, 1),
        ];
        let offset = match base {
            Some(base) => {
                body.statements.extend([
                    literal_binding(9, base),
                    binary_binding(10, BinaryOperation::Add, 9, 8),
                ]);
                10
            }
            None => 8,
        };
        body.statements.push(word_store(offset));
        statements
    }

    /// newyork rewrites `mul(p, 0x20)` into `shl(5, p)`, which from `p = 2` lands on the FMP word.
    #[test]
    fn shifted_counter_store_from_fmp_word_is_unbounded() {
        let statements = shifted_counter_store_loop(2, None);
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "shl(5, p) from p = 2 is 0x40"
        );
    }

    /// A shifted counter above a literal base keeps the base as its minimum, so solc's indexed
    /// stores `add(base, shl(5, i))` keep the range proof; reading the shift from the wrong
    /// operand loses it.
    #[test]
    fn shifted_counter_store_above_fmp_word_stays_bounded() {
        let statements = shifted_counter_store_loop(0, Some(0x80));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "add(0x80, shl(5, p)) never drops below 0x80"
        );
    }

    /// A minimum through `mul` holds only while the exact product fits the word. A counter stepping
    /// by 2^63, multiplied by `u64::MAX` three times, exceeds 2^256 from the fourth iteration on,
    /// and a wrapped product can be any value, so `add(0x80, product)` has no minimum even though
    /// the product's minimum is `0`.
    #[test]
    fn multiplied_counter_store_may_wrap_is_unbounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0, BinaryOperation::Add, BigUint::from(1u64 << 63));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, u64::MAX),
            binary_binding(8, BinaryOperation::Mul, 1, 7),
            binary_binding(9, BinaryOperation::Mul, 8, 7),
            binary_binding(10, BinaryOperation::Mul, 9, 7),
            literal_binding(11, 0x80),
            binary_binding(12, BinaryOperation::Add, 11, 10),
            word_store(12),
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the product can wrap, so 0x80 is no minimum of the sum"
        );
    }

    /// Operations other than `add`, `mul` and `shl` by a static shift have no minimum rule, and
    /// `sub` can take a counter below its seed: `sub(p, 0x40)` from `0x80` starts on the FMP word.
    #[test]
    fn counter_through_other_operation_store_is_unbounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0x40),
            binary_binding(8, BinaryOperation::Sub, 1, 7),
            word_store(8),
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "sub(p, 0x40) from 0x80 is 0x40"
        );
    }

    /// A unary operation has no minimum rule either: `add(not(p), 0x60)`, which is `0x5f - p`,
    /// starts on the FMP word for `p` from `0x1f`.
    #[test]
    fn counter_through_unary_operation_store_is_unbounded() {
        use crate::ir::{BinaryOperation, UnaryOperation, ValueId};
        let mut statements = counter_store_loop(0x1f, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            Statement::Let {
                bindings: vec![ValueId(7)],
                value: Expression::Unary {
                    operation: UnaryOperation::Not,
                    operand: Value::int(ValueId(1)),
                },
            },
            literal_binding(8, 0x60),
            binary_binding(9, BinaryOperation::Add, 7, 8),
            word_store(9),
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "add(not(p), 0x60) from 0x1f is 0x40"
        );
    }

    /// A literal too wide for a static offset, such as `not(0x1f)`, still counts as computed
    /// from literals: `add(p, not(0x1f))` from `0x60` starts on the FMP word.
    #[test]
    fn counter_plus_wide_literal_store_is_unbounded() {
        use crate::ir::{BinaryOperation, Type, ValueId};
        let mut statements = counter_store_loop(0x60, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            Statement::Let {
                bindings: vec![ValueId(7)],
                value: Expression::Literal {
                    value: not_0x1f(),
                    value_type: Type::default(),
                },
            },
            binary_binding(8, BinaryOperation::Add, 1, 7),
            word_store(8),
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "add(p, not(0x1f)) from 0x60 is 0x40"
        );
    }

    /// A store in `post` goes through `post`'s input, which carries the counter's values, so it
    /// is checked like one in the body.
    #[test]
    fn post_block_counter_store_is_unbounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0x40, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, post, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.clear();
        post.statements.insert(0, word_store(4));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "post's input is 0x40 on the first iteration"
        );
    }

    /// The loop output is the counter's last value, so a store through the output of a loop from
    /// `0x40` can land on the FMP word.
    #[test]
    fn output_store_after_counter_from_fmp_word_is_unbounded() {
        use crate::ir::BinaryOperation;
        let mut statements = counter_store_loop(0x40, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.clear();
        statements.push(word_store(6));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the output of a loop from 0x40 can be 0x40"
        );
    }

    /// A `break` hands the output its values, so one that hands on a new value (`0x40`) leaves
    /// the output without the counter's minimum.
    #[test]
    fn break_with_other_counter_value_output_store_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0x40),
            Statement::Break {
                values: vec![Value::int(ValueId(7))],
            },
        ];
        statements.push(word_store(6));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "the output is 0x40 after the break"
        );
    }

    /// A `continue` in a nested loop's body binds to that loop, so the outer counter still steps
    /// up and keeps the range proof.
    #[test]
    fn inner_loop_continue_keeps_outer_counter_bounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.push(nested_loop(vec![Statement::Continue {
            values: vec![Value::int(ValueId(0))],
        }]));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "the inner continue does not hand on the outer counter"
        );
    }

    /// A `break` in a nested loop's body binds to that loop, so the outer loop's output keeps the
    /// outer counter's minimum and a store through it keeps the range proof.
    #[test]
    fn inner_loop_break_keeps_outer_output_bounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x80, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements.push(nested_loop(vec![Statement::Break {
            values: vec![Value::int(ValueId(0))],
        }]));
        statements.push(word_store(6));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "the inner break does not hand on the outer output"
        );
    }

    /// A copy from `0` whose length is a counter from `0x40` covers the FMP word from the second
    /// iteration on, and later iterations write words past the start word, so the pointer is
    /// unbounded and no access may be native.
    #[test]
    fn counter_length_copy_below_fmp_word_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x40, BinaryOperation::Add, BigUint::from(0x20u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0),
            Statement::CallDataCopy {
                destination: Value::int(ValueId(7)),
                offset: Value::int(ValueId(7)),
                length: Value::int(ValueId(1)),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a copy of 0x60 bytes from 0 covers the FMP word"
        );
        assert!(
            results.has_dynamic_accesses,
            "later iterations write words the analysis does not see"
        );
    }

    /// A copy length through `shl` is computed from the counter too: `shl(1, n)` from `0x20`
    /// grows past `0x40`, so a copy from `0` overwrites the FMP word and writes words the
    /// analysis does not see.
    #[test]
    fn shifted_counter_length_copy_is_unbounded() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = counter_store_loop(0x20, BinaryOperation::Add, BigUint::from(1u32));
        let Some(Statement::For { body, .. }) = statements.last_mut() else {
            unreachable!()
        };
        body.statements = vec![
            literal_binding(7, 0),
            literal_binding(8, 1),
            binary_binding(9, BinaryOperation::Shl, 8, 1),
            Statement::CallDataCopy {
                destination: Value::int(ValueId(7)),
                offset: Value::int(ValueId(7)),
                length: Value::int(ValueId(9)),
            },
        ];
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a copy of shl(1, n) bytes from 0 covers the FMP word"
        );
        assert!(
            results.has_dynamic_accesses,
            "later iterations write words the analysis does not see"
        );
    }

    /// Fuzzy dedup turns the literal offsets of `mstore(0x30, v)` and `mstore(0x10, v)` helpers
    /// into a parameter and keeps the `Scratch` tag, which bounds only the first byte, so the merged
    /// store may still overwrite the FMP word and a later `mload(0x40)` must not be truncated.
    #[test]
    fn scratch_word_store_through_parameter_is_unbounded() {
        use crate::ir::{BitWidth, Block, Function, FunctionId, Type, ValueId};
        let mut function = Function::new(FunctionId(0), "store_scratch".to_string());
        function.parameters = vec![
            (ValueId(10), Type::Int(BitWidth::I256)),
            (ValueId(11), Type::Int(BitWidth::I256)),
        ];
        function.body = Block {
            statements: vec![Statement::MStore {
                offset: Value::int(ValueId(10)),
                value: Value::int(ValueId(11)),
                region: MemoryRegion::Scratch,
            }],
        };
        let results = object_with_code(vec![], vec![function]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            results.fmp_could_be_unbounded(),
            "a Scratch-tagged word store through a parameter may reach the FMP word"
        );
    }

    /// A literal FMP below 0x80 breaks the premise that copies to `mload(0x40) + k` miss the FMP
    /// slot, so such a copy could corrupt the pointer unnoticed: the FMP must count as unbounded
    /// so its load is not truncated by the range proof.
    #[test]
    fn fmp_literal_below_dynamic_heap_base_is_untrusted() {
        for pointer in [0x20, 0x7f] {
            let results = object_with_fmp_literal_store(pointer).analyze_heap(TEST_HEAP_SIZE);
            assert!(
                results.fmp_could_be_unbounded(),
                "a literal FMP of {pointer:#x} is below the dynamic heap and must not be trusted"
            );
        }
    }

    /// Builds an object that stores `add(mload(0x40), addend)` to the FMP slot and reads it back.
    fn object_with_fmp_add_store(addend: BigUint) -> Object {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = observe_fmp_statements(0);
        statements.extend([
            big_literal_binding(2, addend),
            binary_binding(3, BinaryOperation::Add, 1, 2),
            Statement::MStore {
                offset: Value::int(ValueId(0)),
                value: Value::int(ValueId(3)),
                region: MemoryRegion::FreePointerSlot,
            },
        ]);
        statements.extend(observe_fmp_statements(4));
        object_with_code(statements, vec![])
    }

    /// Adding a literal of at least the heap size, such as `not(0xff)` (solc's form of subtracting
    /// 0x100), wraps the FMP below its base or moves it out of the heap, so the FMP must count as
    /// unbounded and its load must not be truncated by the range proof.
    #[test]
    fn fmp_add_of_literal_outside_heap_is_untrusted() {
        let not_0xff =
            (BigUint::from(1u32) << revive_common::BIT_LENGTH_WORD) - BigUint::from(0x100u32);
        for addend in [BigUint::from(TEST_HEAP_SIZE), not_0xff] {
            let results = object_with_fmp_add_store(addend.clone()).analyze_heap(TEST_HEAP_SIZE);
            assert!(
                results.fmp_could_be_unbounded(),
                "add(mload(0x40), {addend:#x}) can leave the heap and must not be trusted"
            );
        }
    }

    /// Solidity allocates by storing `add(mload(0x40), size)`, which must keep the range proof.
    #[test]
    fn fmp_add_of_bounded_size_is_trusted() {
        let results =
            object_with_fmp_add_store(BigUint::from(0x20u32)).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "add(mload(0x40), 0x20) is an allocation and must stay trusted"
        );
    }

    /// A non-inlined `finalize_allocation(memPtr, size)` stores `add(memPtr, and(add(size, 31),
    /// not(31)))` with both arguments parameters, so only the literal 31 marks the rounded size
    /// as bounded; it must keep the range proof although 31 is below the dynamic heap base.
    #[test]
    fn fmp_allocation_from_parameters_is_trusted() {
        use crate::ir::{BinaryOperation, ValueId};
        let mut statements = vec![
            literal_binding(2, 31),
            binary_binding(3, BinaryOperation::Add, 1, 2),
            big_literal_binding(4, not_0x1f()),
            binary_binding(5, BinaryOperation::And, 3, 4),
            binary_binding(6, BinaryOperation::Add, 0, 5),
            literal_binding(7, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(7)),
                value: Value::int(ValueId(6)),
                region: MemoryRegion::FreePointerSlot,
            },
        ];
        statements.extend(observe_fmp_statements(8));
        let results = object_with_code(statements, vec![]).analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !results.fmp_could_be_unbounded(),
            "an allocation rounded up with the literal 31 must stay trusted"
        );
    }

    #[test]
    fn test_offset_info_from_literal() {
        let analysis = HeapAnalysis::new(TEST_HEAP_SIZE);

        let expression = Expression::Literal {
            value: BigUint::from(0u32),
            value_type: crate::ir::Type::default(),
        };
        let info = analysis.analyze_expression_offset(&expression).unwrap();
        assert_eq!(info.static_value, Some(0));
        assert_eq!(info.alignment, 32);

        let expression = Expression::Literal {
            value: BigUint::from(64u32),
            value_type: crate::ir::Type::default(),
        };
        let info = analysis.analyze_expression_offset(&expression).unwrap();
        assert_eq!(info.static_value, Some(64));
        assert_eq!(info.alignment, 5);
    }

    /// Fuzzy dedup that parameterizes a literal memory offset invalidates a heap
    /// analysis computed before it runs.
    ///
    /// Two functions identical except for a literal memory offset get merged by
    /// `deduplicate_functions_fuzzy`, which replaces the differing offset literal
    /// with a function parameter. Heap analysis on the *pre-dedup* IR sees only
    /// literal offsets (`has_dynamic_accesses == false`) and would mark those
    /// words native-LE; on the *post-dedup* IR the merged body stores through a
    /// variable offset (`has_dynamic_accesses == true`), which disables native
    /// mode. Lowering with the stale (pre-dedup) result would byte-swap the
    /// parameterized store while literal accesses to the same word stay native-LE
    /// — a byte-order miscompile. This is why `translate_yul_object` must compute
    /// `HeapOptResults` after `run_late_inline_loop`, not before.
    #[test]
    fn fuzzy_dedup_offset_param_invalidates_prior_heap_analysis() {
        use crate::ir::{Block, Function, FunctionId, Object, Type, ValueId};

        fn store_at(offset: u64, offset_id: u32, value_id: u32) -> Vec<Statement> {
            vec![
                Statement::Let {
                    bindings: vec![ValueId(offset_id)],
                    value: Expression::Literal {
                        value: BigUint::from(offset),
                        value_type: Type::default(),
                    },
                },
                Statement::MStore {
                    offset: Value::int(ValueId(offset_id)),
                    value: Value::int(ValueId(value_id)),
                    region: MemoryRegion::from_address(&BigUint::from(offset)),
                },
            ]
        }

        fn make_function(id: u32, offset: u64, offset_id: u32, value_id: u32) -> Function {
            Function {
                id: FunctionId(id),
                name: format!("store_{offset:#x}"),
                parameters: vec![(ValueId(value_id), Type::default())],
                returns: vec![],
                return_values_initial: vec![],
                return_values: vec![],
                body: Block {
                    statements: store_at(offset, offset_id, value_id),
                },
                call_count: 1,
                size_estimate: 15,
            }
        }

        let mut functions = std::collections::BTreeMap::new();
        functions.insert(FunctionId(0), make_function(0, 0x80, 11, 10));
        functions.insert(FunctionId(1), make_function(1, 0xa0, 21, 20));

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements: vec![] },
            functions,
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        };

        let before = object.analyze_heap(TEST_HEAP_SIZE);
        assert!(
            !before.has_dynamic_accesses,
            "pre-dedup: both offsets are literals, so no access is dynamic"
        );

        let removed = crate::simplify::deduplicate_functions_fuzzy(&mut object);
        assert!(
            removed >= 1,
            "the two offset-only-differing functions must fuzzy-merge (offset parameterized)"
        );

        let after = object.analyze_heap(TEST_HEAP_SIZE);
        assert!(
            after.has_dynamic_accesses,
            "post-dedup: the merged body stores through a variable offset parameter"
        );
        assert!(
            !after.can_use_native(0x80),
            "post-dedup native mode must be disabled for the now-variable offset word"
        );
    }
}
