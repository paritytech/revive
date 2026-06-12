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
    for_each_statement, word_align, Block, Expression, MemoryRegion, Object, Statement, Value,
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
    fmp_could_be_unbounded: bool,
    /// Per-`Let`-binding source expression, used by
    /// `is_trusted_fmp_source` to decide whether an mstore's value
    /// comes from a sbrk-style allocator pattern. Only populated for
    /// bindings of size 1.
    value_expressions: BTreeMap<u32, Expression>,
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
                if let Some(addr) = static_offset {
                    self.memory_accesses.insert(addr, pattern);
                } else {
                    self.has_dynamic_accesses = true;
                    // A dynamic full-word store could wrap to the FMP word and corrupt it; we
                    // deliberately do not set `fmp_could_be_unbounded` here — see the field's doc
                    // comment for the (cost-driven) rationale and why no cheap sound fix exists.
                }
                if *region == MemoryRegion::Unknown && !pattern.is_aligned() {
                    if let Some(addr) = static_offset {
                        if addr % BYTE_LENGTH_WORD as u64 != 0 {
                            self.tainted_regions.insert(word_align(addr));
                        }
                    }
                }
                // FMP unboundedness tracking: any `mstore(0x40, value)`
                // whose value isn't a recognized sbrk-allocator pattern
                // means the FMP could hold a non-Solidity-convention
                // value at runtime. Downstream `mload(0x40)` range
                // proofs / native-mode truncations assume `FMP <
                // heap_size`; those assumptions break here.
                let is_fmp_store = region.is_free_pointer_slot(static_offset);
                if is_fmp_store && !self.is_trusted_fmp_source(value.id.0) {
                    self.fmp_could_be_unbounded = true;
                }
            }

            Statement::MStore8 { offset, .. } => {
                let pattern = AccessPattern::Unknown;
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.memory_accesses.insert(addr, pattern);
                    self.tainted_regions.insert(word_align(addr));
                    // A byte write to the FMP word puts non-pointer, big-endian-
                    // positioned data at 0x40. The FMP fast paths (`mload(0x40)`
                    // low-32 native load + the `FMP < heap_size` range proof)
                    // assume a small native pointer there, so they must be
                    // disabled — otherwise `mstore8(0x40,v); mload(0x40)` reads the
                    // wrong half / gets range-masked. Treat the slot as unbounded.
                    if word_align(addr) == 0x40 {
                        self.fmp_could_be_unbounded = true;
                    }
                } else {
                    self.has_dynamic_accesses = true;
                    // A dynamic-offset byte write could land on the FMP word. Unlike the dynamic
                    // full-word `MStore` branch above (which leaves this flag unset for cost
                    // reasons — see the gap note there), dynamic `mstore8` is rare, so flagging it
                    // conservatively costs essentially nothing.
                    self.fmp_could_be_unbounded = true;
                }
            }

            Statement::MCopy { dest, src, length } => {
                let dest_start = self.extract_static_offset(dest);
                let src_start = self.extract_static_offset(src);
                let len = self.extract_static_offset(length);
                self.taint_range(dest_start, len);
                self.taint_range(src_start, len);
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
                // A `return` covering the FMP slot exposes its bytes to the
                // caller in EVM big-endian format. Fire FMP-escape accounting
                // for in-function returns and for any runtime-subobject return;
                // only the deploy (root) object's top-level `return(0, codesize)`
                // is exempt (it returns raw runtime code, not BE-encoded data).
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
                outputs,
                ..
            } => {
                for (initial_value, loop_var) in initial_values.iter().zip(loop_variables.iter()) {
                    if let Some(mut info) = self.offset_values.get(&initial_value.id.0).cloned() {
                        info.from_literal = false;
                        self.offset_values.insert(loop_var.0, info);
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

            Statement::ReturnDataCopy { dest, length, .. } => {
                self.taint_copy_destination(dest, length);
            }

            Statement::CodeCopy { dest, length, .. }
            | Statement::ExtCodeCopy { dest, length, .. }
            | Statement::DataCopy { dest, length, .. }
            | Statement::CallDataCopy { dest, length, .. } => {
                if let Some(addr) = self.extract_static_offset(dest) {
                    self.memory_accesses
                        .entry(addr)
                        .or_insert(AccessPattern::AlignedStatic(addr));
                }
                self.taint_copy_destination(dest, length);
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
            if let Some(static_val) = info.static_value {
                if static_val % BYTE_LENGTH_WORD as u64 == 0 {
                    return AccessPattern::AlignedStatic(static_val);
                } else {
                    return AccessPattern::UnalignedStatic(static_val);
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
            if let Some(static_val) = info.static_value {
                if !info.from_literal {
                    self.variable_accessed_offsets.insert(static_val);
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
                        .map_or(word_start, |prev| prev.min(word_start)),
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
            (Some(addr), Some(size)) => {
                let end = addr.saturating_add(size);
                let first_word = word_align(addr);
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
            (Some(addr), None) => {
                self.escaping_regions.insert(word_align(addr));
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
    /// destination `dest` and length `length`, and flags the free-memory-pointer slot as
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
    fn taint_copy_destination(&mut self, dest: &Value, length: &Value) {
        let dest_start = self.extract_static_offset(dest);
        let len = self.extract_static_offset(length);

        let covers_fmp = match (dest_start, len) {
            (Some(addr), Some(size)) => size > 0 && addr < 0x60 && addr.saturating_add(size) > 0x40,
            (Some(addr), None) => (0x40..0x60).contains(&addr),
            _ => false,
        };
        if covers_fmp {
            self.fmp_could_be_unbounded = true;
        }

        match (dest_start, len) {
            (Some(addr), Some(size)) if size > 0 => self.taint_range(Some(addr), Some(size)),
            (Some(_), Some(_)) => {} // zero-length copy writes nothing
            (Some(addr), None) => {
                self.tainted_regions.insert(word_align(addr));
            }
            (None, _) => self.has_dynamic_accesses = true,
        }
    }

    /// Taints all word-aligned memory regions in a range.
    /// If the range is too large, treats it as a dynamic access instead.
    fn taint_range(&mut self, start: Option<u64>, len: Option<u64>) {
        match (start, len) {
            (Some(addr), Some(size)) if size > 0 => {
                let end = addr.saturating_add(size);
                let first_word = word_align(addr);
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
            (Some(addr), _) => {
                self.tainted_regions.insert(word_align(addr));
                self.has_dynamic_accesses = true;
            }
            (None, _) => {
                self.has_dynamic_accesses = true;
            }
        }
    }

    /// Marks all word-aligned memory regions in a range as both escaping and tainted.
    fn mark_escaping_and_tainted_range(&mut self, offset: &Value, length: &Value) {
        let start = self.extract_static_offset(offset);
        let len = self.extract_static_offset(length);
        match (start, len) {
            (Some(addr), Some(size)) if size > 0 => {
                let end = addr.saturating_add(size);
                let first_word = word_align(addr);
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
            (Some(addr), None) => {
                self.escaping_regions.insert(word_align(addr));
                self.tainted_regions.insert(word_align(addr));
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
                let static_val = if digits.is_empty() {
                    0
                } else if digits.len() == 1 {
                    digits[0]
                } else {
                    return None;
                };
                Some(OffsetInfo {
                    static_value: Some(static_val),
                    alignment: compute_alignment(static_val),
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
                        let lhs_align = lhs_info.map(|i| i.alignment).unwrap_or(1);
                        let rhs_align = rhs_info.map(|i| i.alignment).unwrap_or(1);
                        let result_align = gcd(lhs_align, rhs_align);

                        let static_val = match (
                            lhs_info.and_then(|i| i.static_value),
                            rhs_info.and_then(|i| i.static_value),
                        ) {
                            (Some(l), Some(r)) => Some(l.wrapping_add(r)),
                            _ => None,
                        };

                        Some(OffsetInfo {
                            static_value: static_val,
                            alignment: result_align,
                            from_literal: false,
                        })
                    }

                    crate::ir::BinaryOperation::Mul => {
                        let static_val = match (
                            lhs_info.and_then(|i| i.static_value),
                            rhs_info.and_then(|i| i.static_value),
                        ) {
                            (Some(l), Some(r)) => Some(l.wrapping_mul(r)),
                            _ => None,
                        };

                        let mult_align = match (
                            rhs_info.and_then(|i| i.static_value),
                            lhs_info.and_then(|i| i.static_value),
                        ) {
                            (Some(32), _) | (_, Some(32)) => 32,
                            (Some(n), _) | (_, Some(n)) if n % 32 == 0 => 32,
                            _ => 1,
                        };

                        Some(OffsetInfo {
                            static_value: static_val,
                            alignment: mult_align,
                            from_literal: false,
                        })
                    }

                    crate::ir::BinaryOperation::And => {
                        if let Some(mask) = rhs_info.and_then(|i| i.static_value) {
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
                        if let Some(shift) = rhs_info.and_then(|i| i.static_value) {
                            if shift < 32 {
                                let base_align = lhs_info.map(|i| i.alignment).unwrap_or(1);
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
                if self.extract_static_offset(offset).is_none() {
                    self.has_dynamic_accesses = true;
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
    pub fn requires_big_endian(&self, addr: u64) -> bool {
        let word_addr = word_align(addr);
        self.tainted_regions.contains(&word_addr) || self.escaping_regions.contains(&word_addr)
    }

    /// Returns whether a memory region escapes to external code.
    pub fn region_escapes(&self, addr: u64) -> bool {
        let word_addr = word_align(addr);
        self.escaping_regions.contains(&word_addr)
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
                // No known Let binding for `current`: it's either a
                // function parameter or a result whose Let we haven't
                // visited yet. Either way, we can't prove it's
                // sbrk-bounded.
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
                    // `add(trusted, _)` is trusted iff at least one
                    // operand is trusted. Solidity's allocator emits
                    // `add(mload(0x40), bounded_size)` which fits this.
                    return self.is_trusted_fmp_source(lhs.id.0)
                        || self.is_trusted_fmp_source(rhs.id.0);
                }
                Expression::Binary {
                    operation: crate::ir::BinaryOperation::And,
                    lhs,
                    rhs,
                } => {
                    // `and(trusted, _)` is trusted. `guard_narrow.rs`'s
                    // body-rewrite synthesizes `mstore(0x40, and(sum,
                    // max_const))` for any allocator that matches
                    // `has_allocator_shape`. The `sum` chain leads back
                    // through `add(mload(0x40), aligned_size)`, which
                    // recurses to trusted via the `Add` arm above.
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
            .filter(|p| p.is_aligned())
            .count();
        let static_accesses = self
            .memory_accesses
            .values()
            .filter(|p| p.is_static())
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

impl HeapOptResults {
    /// Creates results from a completed heap analysis.
    pub fn from_analysis(analysis: &HeapAnalysis) -> Self {
        let mut native_safe_regions = BTreeSet::new();
        let mut native_safe_offsets = BTreeSet::new();
        let mut unknown_accesses = 0;

        for (&addr, pattern) in &analysis.memory_accesses {
            if matches!(pattern, AccessPattern::Unknown) {
                unknown_accesses += 1;
            } else if pattern.is_aligned() {
                let word_addr = word_align(addr);
                if !analysis.requires_big_endian(addr) {
                    native_safe_regions.insert(word_addr);
                    native_safe_offsets.insert(addr);
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
        let word_addr = word_align(offset);
        self.native_safe_regions.contains(&word_addr)
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
}
