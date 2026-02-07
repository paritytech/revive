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
}

/// Information about a value used as a memory offset.
#[derive(Clone, Debug)]
pub struct OffsetInfo {
    /// Known static value, if any.
    pub static_value: Option<u64>,
    /// Known alignment (in bytes). 32 means word-aligned.
    pub alignment: u32,
}

impl Default for OffsetInfo {
    fn default() -> Self {
        OffsetInfo {
            static_value: None,
            alignment: 1, // Assume no alignment by default
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
        }
    }

    /// Runs heap analysis on an object.
    pub fn analyze_object(&mut self, object: &Object) {
        // Analyze main code block
        self.analyze_block(&object.code);

        // Analyze all functions
        for function in object.functions.values() {
            self.analyze_block(&function.body);
        }

        // Recursively handle subobjects
        for subobject in &object.subobjects {
            self.analyze_object(subobject);
        }

        // Post-process: determine which regions need big-endian emulation
        self.compute_tainted_regions();
    }

    /// Analyzes a block for memory access patterns.
    fn analyze_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.analyze_statement(stmt);
        }
    }

    /// Analyzes a region for memory access patterns.
    fn analyze_region(&mut self, region: &Region) {
        for stmt in &region.statements {
            self.analyze_statement(stmt);
        }
    }

    /// Analyzes a statement for memory access patterns.
    fn analyze_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let { bindings, value } => {
                // Track offset information for bindings
                if let Some(offset_info) = self.analyze_expr_offset(value) {
                    for binding in bindings {
                        self.offset_values.insert(binding.0, offset_info.clone());
                    }
                }
            }

            Statement::MStore { offset, region, .. } => {
                let pattern = self.classify_access(offset);
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.memory_accesses.insert(addr, pattern);
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
                }
            }

            Statement::MCopy { dest, src, length } => {
                // Memory copies can create complex access patterns
                let dest_pattern = self.classify_access(dest);
                let src_pattern = self.classify_access(src);

                // If source is tainted, destination becomes tainted
                if let Some(src_addr) = self.extract_static_offset(src) {
                    if self.tainted_regions.contains(&(src_addr / 32 * 32)) {
                        if let Some(dest_addr) = self.extract_static_offset(dest) {
                            self.tainted_regions.insert(dest_addr / 32 * 32);
                        }
                    }
                }

                // Unknown length or unaligned access taints everything
                if !dest_pattern.is_aligned() || !src_pattern.is_aligned() {
                    if let Some(dest_addr) = self.extract_static_offset(dest) {
                        self.tainted_regions.insert(dest_addr / 32 * 32);
                    }
                }

                let _ = length; // Length analysis could be more sophisticated
            }

            // Memory escaping to external calls
            Statement::ExternalCall {
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                ..
            } => {
                // Mark input region as escaping
                if let Some(addr) = self.extract_static_offset(args_offset) {
                    self.escaping_regions.insert(addr / 32 * 32);
                }
                // Mark return region as escaping (will be written by external code)
                if let Some(addr) = self.extract_static_offset(ret_offset) {
                    self.escaping_regions.insert(addr / 32 * 32);
                    self.tainted_regions.insert(addr / 32 * 32);
                }
                let _ = (args_length, ret_length);
            }

            Statement::Revert { offset, .. } | Statement::Return { offset, .. } => {
                // Return/revert data escapes to the caller
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.escaping_regions.insert(addr / 32 * 32);
                }
            }

            Statement::Log { offset, .. } => {
                // Log data escapes
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.escaping_regions.insert(addr / 32 * 32);
                }
            }

            Statement::Create { offset, .. } => {
                // Create data escapes
                if let Some(addr) = self.extract_static_offset(offset) {
                    self.escaping_regions.insert(addr / 32 * 32);
                }
            }

            // Recurse into control flow
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                self.analyze_region(then_region);
                if let Some(else_region) = else_region {
                    self.analyze_region(else_region);
                }
            }

            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    self.analyze_region(&case.body);
                }
                if let Some(default) = default {
                    self.analyze_region(default);
                }
            }

            Statement::For { body, post, .. } => {
                self.analyze_region(body);
                self.analyze_region(post);
            }

            Statement::Block(region) => {
                self.analyze_region(region);
            }

            Statement::Expr(expr) => {
                // Check for memory loads that might affect analysis
                self.analyze_expr_side_effects(expr);
            }

            // Data copy operations
            Statement::CodeCopy { dest, .. }
            | Statement::ExtCodeCopy { dest, .. }
            | Statement::ReturnDataCopy { dest, .. }
            | Statement::DataCopy { dest, .. }
            | Statement::CallDataCopy { dest, .. } => {
                // External data copies write big-endian data
                if let Some(addr) = self.extract_static_offset(dest) {
                    self.tainted_regions.insert(addr / 32 * 32);
                }
            }

            // These don't affect memory analysis
            Statement::SStore { .. }
            | Statement::TStore { .. }
            | Statement::SelfDestruct { .. }
            | Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::Leave { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
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
                })
            }

            Expr::Var(id) => self.offset_values.get(&id.0).cloned(),

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
    fn analyze_expr_side_effects(&self, expr: &Expr) {
        // MLoad doesn't have side effects but we track what regions are read
        match expr {
            Expr::MLoad { offset, .. } => {
                let _ = self.classify_access(offset);
            }
            Expr::Keccak256 { offset, .. } => {
                let _ = self.classify_access(offset);
            }
            Expr::Keccak256Pair { .. } | Expr::Keccak256Single { .. } => {
                // Keccak256Pair/Single use scratch memory internally; nothing to classify
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
        }
    }

    /// Checks if a static offset can use native byte order.
    pub fn can_use_native(&self, offset: u64) -> bool {
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
        self.total_accesses > 0
            && self.unknown_accesses == 0
            && self.tainted_count == 0
            && self.escaping_count == 0
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
