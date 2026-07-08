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

use crate::ir::{
    for_each_statement, word_align, Block, Expression, FunctionId, MemoryRegion, Object, Statement,
    Value,
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
    /// **Known gap (deliberate).** A dynamic-offset full-word `MStore` does not set this flag,
    /// even though its offset could wrap (mod 2^256) to the FMP word `[0x40, 0x5f]` and overwrite
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
    /// an immediate frame terminator, so the corrupted pointer is never read back. This flag is
    /// therefore set only for *observed* corruption: [`Self::detect_observed_fmp_corruption`] walks
    /// control flow (see [`Self::scan_fmp_corruption`] and [`Self::fmp_corrupting_functions`]) and
    /// sets it when an overlap store's corruption reaches a later `mload(0x40)`, or a call / error-
    /// string encoding that reads it. The FMP value-forwarding consumer is additionally defended at
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
}

/// Information about a value used as a memory offset.
#[derive(Clone, Debug)]
pub struct OffsetInfo {
    /// Known static value, if any.
    pub static_value: Option<u64>,
    /// Known alignment (in bytes). 32 means word-aligned.
    pub alignment: u32,
    /// Whether this value originates from a literal expression (not a variable).
    /// When false, the LLVM IR value may not be a constant even though the
    /// newyork analysis can resolve it statically.
    pub from_literal: bool,
}

impl Default for OffsetInfo {
    fn default() -> Self {
        OffsetInfo {
            static_value: None,
            alignment: 1,
            from_literal: false,
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
    /// Creates a new heap analysis context.
    pub fn new() -> Self {
        HeapAnalysis {
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
                outputs,
                ..
            } => {
                self.analyze_expression_side_effects(condition);
                for (initial_value, loop_variable) in
                    initial_values.iter().zip(loop_variables.iter())
                {
                    if let Some(mut info) = self.offset_values.get(&initial_value.id.0).cloned() {
                        info.from_literal = false;
                        self.offset_values.insert(loop_variable.0, info);
                    }
                }
                for (initial_value, output) in initial_values.iter().zip(outputs.iter()) {
                    if let Some(mut info) = self.offset_values.get(&initial_value.id.0).cloned() {
                        info.from_literal = false;
                        self.offset_values.insert(output.0, info);
                    }
                }
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

    /// Taints the destination of a copy opcode (`calldatacopy`, `codecopy`,
    /// `returndatacopy`, …), which writes big-endian bytes that a later native
    /// (little-endian) `mload` must not byte-reverse.
    ///
    /// When the length is statically known, every word the copy covers is tainted
    /// — not just the start word — so a multi-word copy can't leave a later word a
    /// native candidate. When the length is dynamic we taint only the start word
    /// and deliberately do NOT set `has_dynamic_accesses`: doing so would disable
    /// native mode for the entire contract (e.g. every ABI-decode `calldatacopy`),
    /// a large code-size regression for no soundness gain over the existing
    /// dynamic-offset guards.
    /// Records the memory tainted by a copy (`calldatacopy`/`codecopy`/`mcopy`/…) with
    /// destination `destination` and length `length`, and flags the free-memory-pointer slot as
    /// possibly unbounded when the copy can clobber it.
    ///
    /// A copy that can overwrite the FMP slot `[0x40, 0x60)` replaces the free-memory
    /// pointer with arbitrary, possibly out-of-range bytes. Downstream codegen that assumes
    /// `FMP < heap_size` (the narrow `mload(0x40)` read and its range proof) would then
    /// mis-read the corrupted value, so such a copy sets `fmp_could_be_unbounded` — exactly
    /// as an untrusted `mstore(0x40, ...)` does. Tainting word 0x40 alone only disables the
    /// *native-mode* FMP read; the FMP *range proof* in `to_llvm` is gated on
    /// `fmp_could_be_unbounded`, so that flag must be set too or the proof silently
    /// truncates the clobbered value.
    ///
    /// `covers_fmp` is kept deliberately narrow to avoid a code-size regression: only a
    /// static destination+length that provably overlap the slot, or a static destination
    /// *inside* the FMP word with a dynamic length (whose first byte(s) land in the slot),
    /// flag unboundedness. A fully-dynamic destination, or a static destination *outside*
    /// the word (proxy `calldatacopy(0, 0, size)`, OZ's FMP-relative ABI-decode copies to
    /// `mload(0x40) >= 0x80`), is left to `has_dynamic_accesses` / the native-mode guards.
    fn taint_copy_destination(&mut self, destination: &Value, length: &Value) {
        let destination_start = self.extract_static_offset(destination);
        let len = self.extract_static_offset(length);

        let covers_fmp = match (destination_start, len) {
            (Some(address), Some(size)) => {
                size > 0 && address < 0x60 && address.saturating_add(size) > 0x40
            }
            (Some(address), None) => (0x40..0x60).contains(&address),
            (None, _) => !self.is_free_pointer_relative(destination.id.0),
        };
        if covers_fmp {
            self.fmp_could_be_unbounded = true;
        }

        match (destination_start, len) {
            (Some(address), Some(size)) => {
                if size > 0 {
                    self.taint_range(Some(address), Some(size));
                }
            }
            (Some(address), None) => {
                self.tainted_regions.insert(word_align(address));
            }
            (None, _) => self.has_dynamic_accesses = true,
        }
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
    /// `add` with such an operand, and `Var` forwarding chains. (The adversarial `add(mload(0x40),
    /// k)` / `add(0x80, k)` that wraps mod 2^256 back to `0x40` is the same solc-unreachable residual
    /// as the dynamic full-word `MStore` gap; see the `fmp_could_be_unbounded` field docs.)
    fn is_free_pointer_relative(&self, value_id: u32) -> bool {
        const MAX_DEPTH: u32 = 32;
        let mut current = value_id;
        for _ in 0..MAX_DEPTH {
            match self.value_expressions.get(&current) {
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
                    return None;
                };
                Some(OffsetInfo {
                    static_value: Some(static_value),
                    alignment: compute_alignment(static_value),
                    from_literal: true,
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

                match operation {
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
                        })
                    }

                    crate::ir::BinaryOperation::And => {
                        if let Some(mask) = rhs_info.and_then(|info| info.static_value) {
                            let align = compute_alignment((!mask).wrapping_add(1));
                            Some(OffsetInfo {
                                static_value: None,
                                alignment: align.max(1),
                                from_literal: false,
                            })
                        } else {
                            None
                        }
                    }

                    crate::ir::BinaryOperation::Shl => {
                        if let Some(shift) = rhs_info.and_then(|info| info.static_value) {
                            if shift < 32 {
                                let base_align = lhs_info.map(|info| info.alignment).unwrap_or(1);
                                Some(OffsetInfo {
                                    static_value: None,
                                    alignment: base_align.saturating_mul(1 << shift),
                                    from_literal: false,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }

                    _ => None,
                }
            }

            Expression::MLoad { .. } => None,

            Expression::CallDataLoad { .. } => None,

            _ => None,
        }
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
    ///   - Literal (`memoryguard(0x80)` collapses to this; any literal
    ///     small enough that `mstore(0x40, <literal>)` would have to
    ///     have been written by the contract author with the allocator
    ///     invariant in mind — we trust them).
    ///   - `Var(x)` where `x` itself is trusted (forwarding chain).
    ///   - `Binary { Add, ... }` where at least one operand is trusted
    ///     (the canonical `add(mload(0x40), bounded_size)` pattern, or
    ///     its variants with both operands derived from FMP arithmetic).
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
        const MAX_FMP_TRUST_DEPTH: u32 = 32;
        let mut current = value_id;
        for _ in 0..MAX_FMP_TRUST_DEPTH {
            let Some(expression) = self.value_expressions.get(&current) else {
                return false;
            };
            match expression {
                Expression::Literal { .. } => return true,
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
                    return self.is_trusted_fmp_source(lhs.id.0)
                        || self.is_trusted_fmp_source(rhs.id.0);
                }
                Expression::Binary {
                    operation: crate::ir::BinaryOperation::And,
                    lhs,
                    rhs,
                } => {
                    return self.is_trusted_fmp_source(lhs.id.0)
                        || self.is_trusted_fmp_source(rhs.id.0);
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

impl Default for HeapAnalysis {
    fn default() -> Self {
        Self::new()
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
    pub fn analyze_heap(&self) -> HeapOptResults {
        let mut analysis = HeapAnalysis::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    /// Builds a `Statement::Let` binding `id` to the literal `value`.
    fn literal(id: u32, value: u64) -> Statement {
        use crate::ir::{Type, ValueId};
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Literal {
                value: BigUint::from(value),
                value_type: Type::default(),
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

    /// Builds a `Let` binding `id` to the literal `value`.
    fn literal_binding(id: u32, value: u64) -> Statement {
        use crate::ir::{Type, ValueId};
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Literal {
                value: BigUint::from(value),
                value_type: Type::default(),
            },
        }
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
        statements.push(literal(20, 0));
        statements.push(literal(21, 23));
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
        let results = object_with_overlap_store(observe_fmp_statements(4)).analyze_heap();
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
        let results = object_with_overlap_store(tail).analyze_heap();
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
        let results = object_with_corrupting_call(observe_fmp_statements(4)).analyze_heap();
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
        let results = object_with_corrupting_call(tail).analyze_heap();
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
        let results = object_with_code(statements, vec![]).analyze_heap();
        assert!(
            results.fmp_could_be_unbounded(),
            "a break skips the loop post, so its FMP restore must not mask the corrupted break path"
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
            literal(2, 0xff),
            Statement::Let {
                bindings: vec![ValueId(10)],
                value: Expression::Binary {
                    operation: BinaryOperation::And,
                    lhs: Value::int(ValueId(1)),
                    rhs: Value::int(ValueId(2)),
                },
            },
        ];
        let results = object_with_dynamic_copy(dest_setup).analyze_heap();
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
            literal(0, 0x40),
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(0)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
            literal(2, 0x20),
            Statement::Let {
                bindings: vec![ValueId(10)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(ValueId(1)),
                    rhs: Value::int(ValueId(2)),
                },
            },
        ];
        let results = object_with_dynamic_copy(dest_setup).analyze_heap();
        assert!(
            !results.fmp_could_be_unbounded(),
            "an add(mload(0x40), k) destination is >= 0x80 and cannot hit the FMP word"
        );
    }

    #[test]
    fn test_offset_info_from_literal() {
        let analysis = HeapAnalysis::new();

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

        let before = object.analyze_heap();
        assert!(
            !before.has_dynamic_accesses,
            "pre-dedup: both offsets are literals, so no access is dynamic"
        );

        let removed = crate::simplify::deduplicate_functions_fuzzy(&mut object);
        assert!(
            removed >= 1,
            "the two offset-only-differing functions must fuzzy-merge (offset parameterized)"
        );

        let after = object.analyze_heap();
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
