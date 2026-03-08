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

use crate::ir::{Block, Expr, MemoryRegion, Object, Region, Statement, Value};

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
    /// Whether any `return` statement (not revert) covers the FMP slot at 0x40.
    /// When true, user data stored at 0x40 could escape via return, making the
    /// FMP native-mode optimization unsafe. Normal Solidity returns from free_ptr
    /// (>= 0x80) so this is only set for inline assembly patterns like `return(0, 96)`.
    has_return_covering_fmp: bool,
    /// Static offsets that are accessed via non-literal (variable) expressions.
    /// When the solc M3 optimizer turns literal offsets into variables
    /// (e.g., `let size := 64; mload(size)`), the LLVM IR value won't be a constant.
    /// Native mode requires LLVM constant detection, so these offsets must use
    /// byte-swap mode to avoid store/load mode mismatches.
    variable_accessed_offsets: BTreeSet<u64>,
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
            alignment: 1, // Assume no alignment by default
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
            has_return_covering_fmp: false,
            variable_accessed_offsets: BTreeSet::new(),
        }
    }

    /// Runs heap analysis on an object.
    pub fn analyze_object(&mut self, object: &Object) {
        // Analyze main code block (constructor — its return(0, size) returns bytecode,
        // not user data, so we don't track FMP return coverage here)
        self.analyze_block(&object.code, false);

        // Analyze all functions (track FMP return coverage here)
        for function in object.functions.values() {
            self.analyze_block(&function.body, true);
        }

        // Recursively handle subobjects.
        // Each subobject has its own ValueId namespace (SSA counters restart from 0),
        // so we must clear the offset_values map to avoid stale lookups that could
        // confuse runtime values (e.g., returndatasize()) with static constants
        // from the parent object that happened to share the same ValueId.
        for subobject in &object.subobjects {
            self.offset_values.clear();
            self.analyze_object(subobject);
        }

        // Post-process: determine which regions need big-endian emulation
        self.compute_tainted_regions();
    }

    /// Analyzes a block for memory access patterns.
    fn analyze_block(&mut self, block: &Block, in_function: bool) {
        for stmt in &block.statements {
            self.analyze_statement(stmt, in_function);
        }
    }

    /// Analyzes a region for memory access patterns.
    fn analyze_region(&mut self, region: &Region, in_function: bool) {
        for stmt in &region.statements {
            self.analyze_statement(stmt, in_function);
        }
    }

    /// Analyzes a statement for memory access patterns.
    fn analyze_statement(&mut self, stmt: &Statement, in_function: bool) {
        match stmt {
            Statement::Let { bindings, value } => {
                // Track offset information for bindings
                if let Some(offset_info) = self.analyze_expr_offset(value) {
                    for binding in bindings {
                        self.offset_values.insert(binding.0, offset_info.clone());
                    }
                }
                // Also check for memory side effects in the expression
                self.analyze_expr_side_effects(value);
            }

            Statement::MStore { offset, region, .. } => {
                let pattern = self.classify_access(offset);
                self.track_variable_access(offset);
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.memory_accesses.insert(addr, pattern);
                } else {
                    self.has_dynamic_accesses = true;
                }
                // If region is known scratch or free pointer, it's more likely aligned
                if *region == MemoryRegion::Unknown && !pattern.is_aligned() {
                    if let Some(addr) = self.extract_static_offset(offset) {
                        // Word-aligned addresses in EVM are at offsets 0, 32, 64, ...
                        if addr % 32 != 0 {
                            self.tainted_regions.insert(addr / 32 * 32);
                        }
                    }
                }
            }

            Statement::MStore8 { offset, .. } => {
                // Single byte stores always create unaligned access
                let pattern = AccessPattern::Unknown;
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.memory_accesses.insert(addr, pattern);
                    self.tainted_regions.insert(addr / 32 * 32);
                } else {
                    self.has_dynamic_accesses = true;
                }
            }

            Statement::MCopy { dest, src, length } => {
                // MCopy transfers raw bytes without byte-swapping.
                // Both source and destination ranges must use big-endian byte order
                // to maintain consistency with surrounding mstore/mload operations.
                let dest_start = self.extract_static_offset(dest);
                let src_start = self.extract_static_offset(src);
                let len = self.extract_static_offset(length);
                // Taint the full destination range
                self.taint_range(dest_start, len);
                // Taint the full source range
                self.taint_range(src_start, len);
            }

            // Memory escaping to external calls
            Statement::ExternalCall {
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                ..
            } => {
                // Mark input region as escaping (full range)
                self.mark_escaping_range(args_offset, args_length);
                // Mark return region as escaping and tainted (written by external code)
                self.mark_escaping_and_tainted_range(ret_offset, ret_length);
            }

            Statement::Revert { offset, length } => {
                // Revert data escapes to the caller — mark all covered regions
                self.mark_escaping_range(offset, length);
            }

            Statement::Return { offset, length } => {
                // Return data escapes to the caller — mark all covered regions
                self.mark_escaping_range(offset, length);
                // Check if this return covers the FMP slot at 0x40.
                // Only check in function bodies — top-level constructor code always
                // has return(0, bytecodeSize) which returns bytecode, not user data
                // (codecopy overwrites 0x40 before the return).
                if in_function {
                    let start = self.extract_static_offset(offset);
                    let len = self.extract_static_offset(length);
                    match (start, len) {
                        (Some(s), Some(l)) => {
                            // Static return: check if it covers the FMP slot
                            if s <= 0x40 && s.saturating_add(l) >= 0x60 {
                                self.has_return_covering_fmp = true;
                            }
                        }
                        (Some(s), None) => {
                            // Known start, dynamic length: the return could cover
                            // any offset from s onwards. Track the minimum start
                            // so we can block native mode for affected offsets.
                            let word_start = s / 32 * 32;
                            self.min_dynamic_escape_start = Some(
                                self.min_dynamic_escape_start
                                    .map_or(word_start, |prev| prev.min(word_start)),
                            );
                        }
                        _ => {
                            // Fully dynamic return (e.g., return(mload(0x40), size)):
                            // Solidity convention means start >= 0x80, so scratch
                            // memory and FMP slot are safe.
                        }
                    }
                }
            }

            Statement::Log { offset, length, .. } => {
                // Log data escapes — mark all covered regions
                self.mark_escaping_range(offset, length);
            }

            Statement::Create { offset, length, .. } => {
                // Create data escapes — mark all covered regions
                self.mark_escaping_range(offset, length);
            }

            // Recurse into control flow
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                self.analyze_region(then_region, in_function);
                if let Some(else_region) = else_region {
                    self.analyze_region(else_region, in_function);
                }
            }

            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    self.analyze_region(&case.body, in_function);
                }
                if let Some(default) = default {
                    self.analyze_region(default, in_function);
                }
            }

            Statement::For {
                init_values,
                loop_vars,
                condition_stmts,
                body,
                post,
                outputs,
                ..
            } => {
                // Loop-carried variables become PHI nodes in LLVM, so they are
                // never constants even if the initial value is a literal.
                // Propagate offset info from init_values but mark as non-literal.
                for (init_val, loop_var) in init_values.iter().zip(loop_vars.iter()) {
                    if let Some(mut info) = self.offset_values.get(&init_val.id.0).cloned() {
                        info.from_literal = false;
                        self.offset_values.insert(loop_var.0, info);
                    }
                }
                // Output variables are also PHI nodes (loop exit values).
                for (init_val, output) in init_values.iter().zip(outputs.iter()) {
                    if let Some(mut info) = self.offset_values.get(&init_val.id.0).cloned() {
                        info.from_literal = false;
                        self.offset_values.insert(output.0, info);
                    }
                }
                for stmt in condition_stmts {
                    self.analyze_statement(stmt, in_function);
                }
                self.analyze_region(body, in_function);
                self.analyze_region(post, in_function);
            }

            Statement::Block(region) => {
                self.analyze_region(region, in_function);
            }

            Statement::Expr(expr) => {
                // Check for memory loads that might affect analysis
                self.analyze_expr_side_effects(expr);
            }

            // ReturnDataCopy writes ABI-encoded big-endian data that needs byte-swapping.
            Statement::ReturnDataCopy { dest, .. } => {
                if let Some(addr) = self.extract_static_offset(dest) {
                    self.tainted_regions.insert(addr / 32 * 32);
                } else {
                    self.has_dynamic_accesses = true;
                }
            }

            // CodeCopy, DataCopy, CallDataCopy, and ExtCodeCopy write external data
            // into memory. When this data is subsequently read via mload, it must be
            // byte-swapped since the source data is in big-endian ABI encoding.
            // Taint the destination to prevent native mode for these regions.
            Statement::CodeCopy { dest, .. }
            | Statement::ExtCodeCopy { dest, .. }
            | Statement::DataCopy { dest, .. }
            | Statement::CallDataCopy { dest, .. } => {
                if let Some(addr) = self.extract_static_offset(dest) {
                    self.memory_accesses
                        .entry(addr)
                        .or_insert(AccessPattern::AlignedStatic(addr));
                    self.tainted_regions.insert(addr / 32 * 32);
                } else {
                    self.has_dynamic_accesses = true;
                }
            }

            // These don't affect memory analysis
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
        // Check if we know this value's offset info
        if let Some(info) = self.offset_values.get(&offset.id.0) {
            if let Some(static_val) = info.static_value {
                if static_val % 32 == 0 {
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
    fn mark_escaping_range(&mut self, offset: &Value, length: &Value) {
        let start = self.extract_static_offset(offset);
        let len = self.extract_static_offset(length);
        match (start, len) {
            (Some(_), Some(0)) => {
                // Zero-length range: nothing escapes
            }
            (Some(addr), Some(size)) => {
                let end = addr.saturating_add(size);
                let first_word = addr / 32 * 32;
                let range = end.saturating_sub(first_word);
                let num_words = range.saturating_add(31) / 32;
                if num_words > MAX_RANGE_WORDS {
                    // Range too large to enumerate; treat as dynamic escape
                    self.escaping_regions.insert(first_word);
                    self.has_dynamic_escapes = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.escaping_regions.insert(word);
                        word += 32;
                    }
                }
            }
            (Some(addr), None) => {
                self.escaping_regions.insert(addr / 32 * 32);
                self.has_dynamic_escapes = true;
            }
            (None, _) => {
                self.has_dynamic_escapes = true;
            }
        }
    }

    /// Taints all word-aligned memory regions in a range.
    /// If the range is too large, treats it as a dynamic access instead.
    fn taint_range(&mut self, start: Option<u64>, len: Option<u64>) {
        match (start, len) {
            (Some(addr), Some(size)) if size > 0 => {
                let end = addr.saturating_add(size);
                let first_word = addr / 32 * 32;
                let num_words = end.saturating_sub(first_word).saturating_add(31) / 32;
                if num_words > MAX_RANGE_WORDS {
                    self.tainted_regions.insert(first_word);
                    self.has_dynamic_accesses = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.tainted_regions.insert(word);
                        word += 32;
                    }
                }
            }
            (Some(addr), _) => {
                self.tainted_regions.insert(addr / 32 * 32);
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
                let first_word = addr / 32 * 32;
                let num_words = end.saturating_sub(first_word).saturating_add(31) / 32;
                if num_words > MAX_RANGE_WORDS {
                    self.escaping_regions.insert(first_word);
                    self.tainted_regions.insert(first_word);
                    self.has_dynamic_escapes = true;
                } else {
                    let mut word = first_word;
                    while word < end {
                        self.escaping_regions.insert(word);
                        self.tainted_regions.insert(word);
                        word += 32;
                    }
                }
            }
            (Some(addr), None) => {
                self.escaping_regions.insert(addr / 32 * 32);
                self.tainted_regions.insert(addr / 32 * 32);
                self.has_dynamic_escapes = true;
            }
            _ => {
                self.has_dynamic_escapes = true;
            }
        }
    }

    /// Analyzes an expression to extract offset information.
    fn analyze_expr_offset(&self, expr: &Expr) -> Option<OffsetInfo> {
        match expr {
            Expr::Literal { value, .. } => {
                let digits = value.to_u64_digits();
                let static_val = if digits.is_empty() {
                    0
                } else if digits.len() == 1 {
                    digits[0]
                } else {
                    // Value is too large
                    return None;
                };
                Some(OffsetInfo {
                    static_value: Some(static_val),
                    alignment: compute_alignment(static_val),
                    from_literal: true,
                })
            }

            Expr::Var(id) => self.offset_values.get(&id.0).cloned().map(|mut info| {
                // A variable reference may not be a constant in LLVM IR,
                // even if the newyork analysis knows its static value.
                info.from_literal = false;
                info
            }),

            Expr::Binary { op, lhs, rhs } => {
                let lhs_info = self.offset_values.get(&lhs.id.0);
                let rhs_info = self.offset_values.get(&rhs.id.0);

                match op {
                    crate::ir::BinOp::Add => {
                        // Adding two values: alignment is GCD of alignments
                        let lhs_align = lhs_info.map(|i| i.alignment).unwrap_or(1);
                        let rhs_align = rhs_info.map(|i| i.alignment).unwrap_or(1);
                        let result_align = gcd(lhs_align, rhs_align);

                        // If both static, compute result
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

                    crate::ir::BinOp::Mul => {
                        // Multiplying by constant affects alignment
                        let static_val = match (
                            lhs_info.and_then(|i| i.static_value),
                            rhs_info.and_then(|i| i.static_value),
                        ) {
                            (Some(l), Some(r)) => Some(l.wrapping_mul(r)),
                            _ => None,
                        };

                        // If multiplying by 32, alignment is at least 32
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

                    crate::ir::BinOp::And => {
                        // AND with mask can improve alignment knowledge
                        // e.g., x & 0xFFFFFFE0 ensures 32-byte alignment
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

                    crate::ir::BinOp::Shl => {
                        // Shift left increases alignment
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

            // MLoad returns memory content, not useful for offset analysis
            Expr::MLoad { .. } => None,

            // CallDataLoad could be any value
            Expr::CallDataLoad { .. } => None,

            _ => None,
        }
    }

    /// Analyzes expression side effects on memory.
    fn analyze_expr_side_effects(&mut self, expr: &Expr) {
        // MLoad doesn't have side effects but we track what regions are read
        match expr {
            Expr::MLoad { offset, .. } => {
                let _ = self.classify_access(offset);
                self.track_variable_access(offset);
                if self.extract_static_offset(offset).is_none() {
                    self.has_dynamic_accesses = true;
                }
            }
            Expr::Keccak256 { offset, length } => {
                let _ = self.classify_access(offset);
                if self.extract_static_offset(offset).is_none() {
                    self.has_dynamic_accesses = true;
                }
                // Keccak256 reads raw bytes from memory, so the memory region
                // must be in big-endian format. Mark it as escaping.
                self.mark_escaping_range(offset, length);
            }
            Expr::Keccak256Pair { .. } | Expr::Keccak256Single { .. } => {
                // Keccak256Pair/Single use scratch memory internally; nothing to classify
            }
            Expr::MappingSLoad { .. } => {
                // MappingSLoad is a compound keccak256+sload; no heap memory effects
            }
            _ => {}
        }
    }

    /// Computes which regions need big-endian emulation.
    fn compute_tainted_regions(&mut self) {
        // Regions that escape always need big-endian for EVM compatibility
        for &region in &self.escaping_regions {
            self.tainted_regions.insert(region);
        }
    }

    /// Returns whether a memory region requires big-endian emulation.
    pub fn requires_big_endian(&self, addr: u64) -> bool {
        let word_addr = addr / 32 * 32;
        self.tainted_regions.contains(&word_addr) || self.escaping_regions.contains(&word_addr)
    }

    /// Returns whether a memory region escapes to external code.
    pub fn region_escapes(&self, addr: u64) -> bool {
        let word_addr = addr / 32 * 32;
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
    pub fn has_return_covering_fmp(&self) -> bool {
        self.has_return_covering_fmp
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
    has_return_covering_fmp: bool,
    /// Static offsets that are accessed via non-literal (variable) expressions.
    /// These offsets may not be LLVM constants, so native mode would cause
    /// a store/load mode mismatch with literal accesses to the same offset.
    variable_accessed_offsets: BTreeSet<u64>,
}

impl HeapOptResults {
    /// Creates results from a completed heap analysis.
    pub fn from_analysis(analysis: &HeapAnalysis) -> Self {
        let mut native_safe_regions = BTreeSet::new();
        let mut native_safe_offsets = BTreeSet::new();
        let mut unknown_accesses = 0;

        // Find all accessed addresses that are aligned and not tainted/escaping
        for (&addr, pattern) in &analysis.memory_accesses {
            if matches!(pattern, AccessPattern::Unknown) {
                unknown_accesses += 1;
            } else if pattern.is_aligned() {
                let word_addr = addr / 32 * 32;
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
            has_return_covering_fmp: analysis.has_return_covering_fmp(),
            variable_accessed_offsets: analysis.variable_accessed_offsets().clone(),
        }
    }

    /// Checks if a static offset can use native byte order.
    pub fn can_use_native(&self, offset: u64) -> bool {
        // When a static offset is also accessed via a non-literal expression
        // (e.g., `let size := 64; mload(size)`), LLVM may not see it as a constant.
        // The literal access would get InlineNative, but the variable access would
        // get ByteSwap, causing a store/load mode mismatch. Disable native mode.
        if self.variable_accessed_offsets.contains(&offset) {
            return false;
        }
        // When there are dynamic-length return statements with known start,
        // any offset at or above that start could escape via the return.
        if let Some(min_start) = self.min_dynamic_escape_start {
            let word_offset = offset / 32 * 32;
            if word_offset >= min_start {
                return false;
            }
        }
        // When there are fully dynamic escapes (e.g., return(mload(0x40), size)),
        // any region beyond the Solidity reserved area could potentially escape.
        // The Solidity free memory pointer starts at 0x80, so only offsets in the
        // reserved area (0x00-0x5F) are provably safe. The FMP slot (0x40-0x5F)
        // is handled separately by the fmp_native_safe() check in native_memory_mode().
        if self.has_dynamic_escapes && offset >= 0x60 {
            return false;
        }
        // Check if this specific offset is safe
        if self.native_safe_offsets.contains(&offset) {
            return true;
        }
        // Check if the word region is safe
        let word_addr = offset / 32 * 32;
        self.native_safe_regions.contains(&word_addr)
    }

    /// Returns true if any optimization opportunities were found.
    pub fn has_optimizations(&self) -> bool {
        !self.native_safe_regions.is_empty()
    }

    /// Returns true if the FMP slot at 0x40 is safe for native-mode optimization.
    /// This is false when:
    /// - A static `return` covers offset 0x40 (e.g., `return(0, 96)`)
    /// - A dynamic-length escape starts at or before 0x40 (e.g., `return(0, dynamic)`)
    /// - Offset 0x40 is accessed via a non-literal expression (LLVM won't see a constant)
    pub fn fmp_native_safe(&self) -> bool {
        // If 0x40 is accessed via a variable, LLVM won't see a constant and will
        // use ByteSwap mode, mismatching with literal InlineNative stores.
        if self.variable_accessed_offsets.contains(&0x40) {
            return false;
        }
        if self.has_return_covering_fmp {
            return false;
        }
        // A dynamic-length escape starting at <= 0x40 could cover the FMP slot
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
        // Conditions for native-only mode:
        // 1. We must have analyzed at least one access
        // 2. No unknown/dynamic accesses
        // 3. No tainted regions (unaligned writes)
        // 4. No escaping regions (external interfaces)
        // 5. No dynamic escapes (return/revert/call/log/create with unresolved offsets)
        // 6. No dynamic memory accesses (mstore/mload with unresolved offsets)
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
        return 32; // Zero is aligned to everything
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
        assert_eq!(compute_alignment(1), 0); // 2^0 = 1
        assert_eq!(compute_alignment(2), 1); // 2^1 = 2
        assert_eq!(compute_alignment(4), 2); // 2^2 = 4
        assert_eq!(compute_alignment(32), 5); // 2^5 = 32
        assert_eq!(compute_alignment(64), 5); // capped at 32
        assert_eq!(compute_alignment(33), 0); // odd
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

        // Zero literal
        let expr = Expr::Literal {
            value: BigUint::from(0u32),
            ty: crate::ir::Type::default(),
        };
        let info = analysis.analyze_expr_offset(&expr).unwrap();
        assert_eq!(info.static_value, Some(0));
        assert_eq!(info.alignment, 32); // Zero is maximally aligned

        // Word-aligned literal (0x40 = 64)
        let expr = Expr::Literal {
            value: BigUint::from(64u32),
            ty: crate::ir::Type::default(),
        };
        let info = analysis.analyze_expr_offset(&expr).unwrap();
        assert_eq!(info.static_value, Some(64));
        // 64 = 2^6, so alignment is capped at 32
        assert_eq!(info.alignment, 5);
    }
}
