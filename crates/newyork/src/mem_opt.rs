//! Memory optimization pass for load-after-store elimination and dead store elimination.
//!
//! This module provides the infrastructure for two key optimizations:
//!
//! 1. **Load-after-store elimination**: If we just stored a value to a memory location
//!    and then load from that same location, we can reuse the stored value directly
//!    without accessing memory.
//!
//! 2. **Dead store elimination**: If we store a value to a memory location and that
//!    value is never read before being overwritten or the function returns, we can
//!    eliminate the store.
//!
//! # Current Status
//!
//! The infrastructure for tracking memory state and constant values is in place. The pass
//! correctly traverses the IR and maintains state, but currently applies **conservative**
//! state clearing at control flow boundaries. This ensures correctness while we build
//! confidence in the implementation.
//!
//! The optimization will fire when:
//! - A load immediately follows a store to the same static offset
//! - Both offsets can be resolved to compile-time constants
//!
//! # Memory Model
//!
//! We track memory state at the granularity of 32-byte words (matching EVM semantics).
//! A store to a known static offset records the stored value at that offset.
//! A store to an unknown (dynamic) offset conservatively invalidates all tracked state.
//!
//! # Value Tracking
//!
//! We track constant values through `Let` bindings to determine static memory offsets:
//! - `Let { x, Literal(42) }` - x is known to be 42
//! - `Let { y, Binary(Add, x, z) }` where x=10, z=5 - y is known to be 15
//!
//! This enables us to identify when memory operations use constant offsets even when
//! the offset is computed through intermediate variables.
//!
//! # Safety
//!
//! The pass is conservative at control flow joins - we clear all state when entering
//! or exiting control flow constructs (if, switch, for, block). This ensures correctness
//! but limits optimization opportunities. Future work can implement proper state merging
//! for branches.
//!
//! # Limitations
//!
//! - Only static offsets (compile-time constants) are tracked
//! - Cross-function optimization is not implemented
//! - Control flow joins clear all state (conservative but correct)
//! - Dead store elimination is tracked but not applied yet

use std::collections::{BTreeMap, BTreeSet};

use num::{BigUint, Zero};

use crate::ir::{
    for_each_statement, word_align, word_store_overlaps_free_pointer_slot, BinaryOperation,
    BitWidth, Block, Expression, FunctionId, MemoryRegion, Object, Region, Statement, Type, Value,
    ValueId,
};
use revive_common::BYTE_LENGTH_WORD;

/// Results of memory optimization.
#[derive(Clone, Debug, Default)]
pub struct MemOptResults {
    /// Number of loads eliminated (replaced with stored value).
    pub loads_eliminated: usize,
    /// Number of stores eliminated (dead stores).
    pub stores_eliminated: usize,
    /// Number of values tracked.
    pub values_tracked: usize,
    /// Number of keccak256 calls fused into keccak256_pair.
    pub keccak_pairs_fused: usize,
    /// Number of keccak256 calls fused into keccak256_single.
    pub keccak_singles_fused: usize,
    /// Number of FMP (`mload(0x40)`) loads eliminated by [`FmpPropagation`].
    pub fmp_loads_eliminated: usize,
}

impl std::ops::AddAssign for MemOptResults {
    fn add_assign(&mut self, rhs: Self) {
        self.loads_eliminated += rhs.loads_eliminated;
        self.stores_eliminated += rhs.stores_eliminated;
        self.values_tracked += rhs.values_tracked;
        self.keccak_pairs_fused += rhs.keccak_pairs_fused;
        self.keccak_singles_fused += rhs.keccak_singles_fused;
        self.fmp_loads_eliminated += rhs.fmp_loads_eliminated;
    }
}

/// Memory optimization pass.
pub struct MemoryOptimizer {
    /// Tracks the most recently stored value at each static memory offset.
    /// Key is the word-aligned offset.
    memory_state: BTreeMap<u64, TrackedValue>,
    /// Tracks constant values for ValueIds.
    /// When a Let binds a literal, we record the constant value here.
    constant_values: BTreeMap<u32, BigUint>,
    /// Counter for fresh value IDs when creating new bindings.
    next_value_id: u32,
    /// Statistics about optimizations performed.
    statistics: MemOptResults,
    /// Indices of dead stores that should be removed in the current statement list.
    /// A store is dead if it's overwritten before being read.
    dead_store_indices: BTreeSet<usize>,
    /// Pending stores that haven't been read yet.
    /// Maps word-aligned offset to the index in the current statement list.
    pending_stores: BTreeMap<u64, usize>,
}

/// A tracked value in memory.
#[derive(Clone, Debug)]
struct TrackedValue {
    /// The value that was stored.
    stored_value: Value,
    /// The exact offset where it was stored.
    offset: u64,
    /// Whether this store has been read (for dead store elimination).
    was_read: bool,
}

impl MemoryOptimizer {
    /// Creates a new memory optimizer.
    pub fn new() -> Self {
        MemoryOptimizer {
            memory_state: BTreeMap::new(),
            constant_values: BTreeMap::new(),
            next_value_id: 0,
            statistics: MemOptResults::default(),
            dead_store_indices: BTreeSet::new(),
            pending_stores: BTreeMap::new(),
        }
    }

    /// Optimizes an object in place.
    pub fn optimize_object(&mut self, object: &mut Object) -> MemOptResults {
        self.next_value_id = object.find_max_value_id() + 1;

        self.optimize_block(&mut object.code);

        for function in object.functions.values_mut() {
            self.memory_state.clear();
            self.constant_values.clear();
            self.optimize_block(&mut function.body);
        }

        std::mem::take(&mut self.statistics)
    }

    /// Optimizes a block in place.
    fn optimize_block(&mut self, block: &mut Block) {
        let statements = std::mem::take(&mut block.statements);
        block.statements = self.optimize_statements(statements);
    }

    /// Optimizes a region in place.
    fn optimize_region(&mut self, region: &mut Region) {
        let statements = std::mem::take(&mut region.statements);
        region.statements = self.optimize_statements(statements);
    }

    /// Optimizes a list of statements.
    fn optimize_statements(&mut self, statements: Vec<Statement>) -> Vec<Statement> {
        let outer_dead_stores = std::mem::take(&mut self.dead_store_indices);
        let outer_pending = std::mem::take(&mut self.pending_stores);

        let mut processed = Vec::with_capacity(statements.len());

        for (index, statement) in statements.into_iter().enumerate() {
            match statement {
                Statement::MStore {
                    offset,
                    value,
                    region,
                } => {
                    if let Some(static_offset) = self.try_get_static_offset(&offset) {
                        let word_offset = word_align(static_offset);

                        self.eliminate_dead_store_at(static_offset, index);
                        self.invalidate_state_overlapping_store(static_offset);

                        self.pending_stores.insert(static_offset, index);

                        self.memory_state.insert(
                            word_offset,
                            TrackedValue {
                                stored_value: value,
                                offset: static_offset,
                                was_read: false,
                            },
                        );
                        self.statistics.values_tracked += 1;
                    } else {
                        self.memory_state.clear();
                        self.pending_stores.clear();
                    }
                    processed.push(Statement::MStore {
                        offset,
                        value,
                        region,
                    });
                }

                Statement::MStore8 {
                    offset,
                    value,
                    region,
                } => {
                    if let Some(static_offset) = self.try_get_static_offset(&offset) {
                        self.invalidate_state_covering_byte(static_offset);
                    } else {
                        self.memory_state.clear();
                        self.pending_stores.clear();
                    }
                    processed.push(Statement::MStore8 {
                        offset,
                        value,
                        region,
                    });
                }

                Statement::MCopy { dest, src, length } => {
                    self.memory_state.clear();
                    self.pending_stores.clear();
                    processed.push(Statement::MCopy { dest, src, length });
                }

                Statement::Let { bindings, value } => {
                    let is_call = matches!(&value, Expression::Call { .. });

                    let optimized_value = self.optimize_expr_with_read_tracking(value);

                    if is_call {
                        self.memory_state.clear();
                        self.pending_stores.clear();
                    }

                    if bindings.len() == 1 {
                        self.record_constant(bindings[0], &optimized_value);
                    }

                    processed.push(Statement::Let {
                        bindings,
                        value: optimized_value,
                    });
                }

                Statement::If {
                    condition,
                    inputs,
                    mut then_region,
                    else_region,
                    outputs,
                } => {
                    self.pending_stores.clear();

                    let pre_branch_memory = self.memory_state.clone();
                    let pre_branch_constants = self.constant_values.clone();

                    self.optimize_region(&mut then_region);
                    let then_memory = self.memory_state.clone();
                    let then_constants = self.constant_values.clone();

                    let else_region = if let Some(mut er) = else_region {
                        self.memory_state = pre_branch_memory;
                        self.constant_values = pre_branch_constants;
                        self.optimize_region(&mut er);

                        self.memory_state =
                            Self::intersect_memory_state(&then_memory, &self.memory_state);
                        self.constant_values =
                            Self::intersect_constants(&then_constants, &self.constant_values);
                        Some(er)
                    } else {
                        self.memory_state =
                            Self::intersect_memory_state(&then_memory, &pre_branch_memory);
                        self.constant_values =
                            Self::intersect_constants(&then_constants, &pre_branch_constants);
                        None
                    };

                    processed.push(Statement::If {
                        condition,
                        inputs,
                        then_region,
                        else_region,
                        outputs,
                    });
                }

                Statement::Switch {
                    scrutinee,
                    inputs,
                    mut cases,
                    default,
                    outputs,
                } => {
                    self.pending_stores.clear();

                    let pre_branch_memory = self.memory_state.clone();
                    let pre_branch_constants = self.constant_values.clone();

                    let mut exit_memories = Vec::new();
                    let mut exit_constants = Vec::new();
                    for case in &mut cases {
                        self.memory_state = pre_branch_memory.clone();
                        self.constant_values = pre_branch_constants.clone();
                        self.optimize_region(&mut case.body);
                        exit_memories.push(self.memory_state.clone());
                        exit_constants.push(self.constant_values.clone());
                    }

                    let default = if let Some(mut d) = default {
                        self.memory_state = pre_branch_memory;
                        self.constant_values = pre_branch_constants;
                        self.optimize_region(&mut d);
                        exit_memories.push(self.memory_state.clone());
                        exit_constants.push(self.constant_values.clone());
                        Some(d)
                    } else {
                        exit_memories.push(pre_branch_memory);
                        exit_constants.push(pre_branch_constants);
                        None
                    };

                    self.memory_state = Self::intersect_memory_states(&exit_memories);
                    self.constant_values = Self::intersect_all_constants(&exit_constants);

                    processed.push(Statement::Switch {
                        scrutinee,
                        inputs,
                        cases,
                        default,
                        outputs,
                    });
                }

                Statement::For {
                    initial_values,
                    loop_variables,
                    mut condition_statements,
                    condition,
                    mut body,
                    post_input_variables,
                    mut post,
                    outputs,
                } => {
                    self.memory_state.clear();
                    self.constant_values.clear();
                    self.pending_stores.clear();

                    condition_statements = self.optimize_statements(condition_statements);
                    self.memory_state.clear();
                    self.constant_values.clear();
                    self.optimize_region(&mut body);
                    self.memory_state.clear();
                    self.constant_values.clear();
                    self.optimize_region(&mut post);

                    self.memory_state.clear();
                    self.constant_values.clear();

                    processed.push(Statement::For {
                        initial_values,
                        loop_variables,
                        condition_statements,
                        condition,
                        body,
                        post_input_variables,
                        post,
                        outputs,
                    });
                }

                Statement::Block(mut region) => {
                    self.pending_stores.clear();
                    self.optimize_region(&mut region);
                    processed.push(Statement::Block(region));
                }

                Statement::Expression(expression) => {
                    let is_call = matches!(&expression, Expression::Call { .. });

                    let optimized = self.optimize_expr_with_read_tracking(expression);

                    if is_call {
                        self.memory_state.clear();
                        self.pending_stores.clear();
                    }

                    processed.push(Statement::Expression(optimized));
                }

                Statement::ExternalCall { .. } => {
                    self.memory_state.clear();
                    self.pending_stores.clear();
                    processed.push(statement);
                }

                Statement::Create { .. } => {
                    self.memory_state.clear();
                    self.pending_stores.clear();
                    processed.push(statement);
                }

                Statement::CodeCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::ExtCodeCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::ReturnDataCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::DataCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::CallDataCopy {
                    ref dest,
                    ref length,
                    ..
                } => {
                    self.invalidate_state_for_copy(dest, length);
                    processed.push(statement);
                }

                Statement::Log {
                    offset,
                    length,
                    topics,
                } => {
                    self.pending_stores.clear();
                    processed.push(Statement::Log {
                        offset,
                        length,
                        topics,
                    });
                }

                Statement::Return { .. } | Statement::Revert { .. } => {
                    self.pending_stores.clear();
                    processed.push(statement);
                }

                Statement::SStore { .. }
                | Statement::TStore { .. }
                | Statement::MappingSStore { .. }
                | Statement::SetImmutable { .. }
                | Statement::SelfDestruct { .. }
                | Statement::Stop
                | Statement::Invalid
                | Statement::PanicRevert { .. }
                | Statement::ErrorStringRevert { .. }
                | Statement::CustomErrorRevert { .. }
                | Statement::Break { .. }
                | Statement::Continue { .. }
                | Statement::Leave { .. } => {
                    processed.push(statement);
                }
            }
        }

        let result = if self.dead_store_indices.is_empty() {
            processed
        } else {
            processed
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !self.dead_store_indices.contains(index))
                .map(|(_, statement)| statement)
                .collect()
        };

        self.dead_store_indices = outer_dead_stores;
        self.pending_stores = outer_pending;

        result
    }

    /// Marks an unread pending store to `static_offset` as dead when the current
    /// `mstore` at the same offset fully overwrites it.
    ///
    /// Must run before [`Self::invalidate_state_overlapping_store`], which drops the
    /// matching `pending_stores` entry that this dead-store check relies on.
    fn eliminate_dead_store_at(&mut self, static_offset: u64, index: usize) {
        if let Some(&prev_idx) = self.pending_stores.get(&static_offset) {
            self.dead_store_indices.insert(prev_idx);
            self.statistics.stores_eliminated += 1;
            log::trace!(
                "Dead store at index {} (offset {}) - overwritten at index {}",
                prev_idx,
                static_offset,
                index
            );
        }
    }

    /// Invalidates tracked state whose 32-byte write range overlaps the word written
    /// by an `mstore(static_offset, v)`.
    ///
    /// An `mstore(p, v)` writes 32 bytes at `[p, p + 32)`. Forwarding compares the
    /// exact `tracked.offset` against the load's static offset, so any tracked entry
    /// whose own 32-byte write range overlaps `[p, p + 32)` must be invalidated —
    /// otherwise a later `mload` at the stale tracked offset would forward the
    /// pre-overwrite value while the actual memory has been partially overwritten by
    /// the new store. A partial overwrite does not make a prior `pending_stores`
    /// entry dead (its bytes are only partially covered), but a later `mload` won't
    /// match the original offset either, so the pending entry can never be matched
    /// and is dropped.
    fn invalidate_state_overlapping_store(&mut self, static_offset: u64) {
        let new_end = static_offset.saturating_add(BYTE_LENGTH_WORD as u64);
        self.memory_state.retain(|_, tracked| {
            let tracked_end = tracked.offset.saturating_add(BYTE_LENGTH_WORD as u64);
            tracked_end <= static_offset || tracked.offset >= new_end
        });
        self.pending_stores.retain(|&k, _| {
            let prev_end = k.saturating_add(BYTE_LENGTH_WORD as u64);
            prev_end <= static_offset || k >= new_end
        });
    }

    /// Invalidates tracked state whose 32-byte write range covers the single byte
    /// written by an `mstore8(static_offset, _)`.
    ///
    /// `mstore8(p, _)` overwrites the single byte at `p`. Any tracked entry from an
    /// earlier `mstore` whose 32-byte write range `[tracked.offset, tracked.offset + 32)`
    /// covers `p` becomes stale — `mload` at the still-cached exact tracked offset
    /// would forward the pre-overwrite value. Every such entry and the matching
    /// `pending_stores` key is invalidated.
    fn invalidate_state_covering_byte(&mut self, static_offset: u64) {
        self.memory_state.retain(|_, tracked| {
            let tracked_end = tracked.offset.saturating_add(BYTE_LENGTH_WORD as u64);
            tracked.offset > static_offset || tracked_end <= static_offset
        });
        self.pending_stores.retain(|&k, _| {
            let prev_end = k.saturating_add(BYTE_LENGTH_WORD as u64);
            k > static_offset || prev_end <= static_offset
        });
    }

    /// Invalidates tracked state overlapping the range written by a `*copy(dest, _, length)`.
    ///
    /// A `*copy(dest, _, length)` writes `length` bytes at `[dest, dest + length)`. Any
    /// tracked entry whose own 32-byte write range overlaps that range must be
    /// invalidated — otherwise a later `mload` at the still-cached exact tracked offset
    /// would forward the pre-overwrite value while actual memory has been overwritten by
    /// the copy. Three cases are handled, in priority order:
    ///
    /// 1. `length == Some(0)`: zero-byte copy is a no-op regardless of whether `dest` is
    ///    known — nothing to invalidate.
    /// 2. `dest == Some(start)` and `length == Some(size)` with `size > 0`: known write
    ///    range, retain only entries that don't overlap.
    /// 3. anything else (dynamic dest with nonzero or unknown length, dynamic length with
    ///    known dest): conservatively clear all tracking.
    fn invalidate_state_for_copy(&mut self, dest: &Value, length: &Value) {
        let copy_length = self.try_get_static_offset(length);
        let copy_dest = self.try_get_static_offset(dest);
        if matches!(copy_length, Some(0)) {
        } else if let (Some(start), Some(size)) = (copy_dest, copy_length) {
            let end = start.saturating_add(size);
            self.memory_state.retain(|_, tracked| {
                let tracked_end = tracked.offset.saturating_add(BYTE_LENGTH_WORD as u64);
                tracked_end <= start || tracked.offset >= end
            });
            self.pending_stores.retain(|&k, _| {
                let prev_end = k.saturating_add(BYTE_LENGTH_WORD as u64);
                prev_end <= start || k >= end
            });
        } else {
            self.memory_state.clear();
            self.pending_stores.clear();
        }
    }

    /// Tracks memory reads for dead store elimination and load-after-store forwarding.
    ///
    /// When a load follows a store to the same static offset, the stored value is
    /// forwarded directly, eliminating the redundant memory round-trip. If the stored
    /// value has a narrower type than I256 (what MLoad produces), a ZeroExtend is
    /// inserted to maintain type correctness.
    fn optimize_expr_with_read_tracking(&mut self, expression: Expression) -> Expression {
        match expression {
            Expression::MLoad { offset, region } => {
                if let Some(static_offset) = self.try_get_static_offset(&offset) {
                    let word_offset = word_align(static_offset);

                    self.pending_stores.remove(&word_offset);

                    if let Some(tracked) = self.memory_state.get_mut(&word_offset) {
                        if tracked.offset == static_offset {
                            tracked.was_read = true;

                            let stored = tracked.stored_value;
                            self.statistics.loads_eliminated += 1;
                            log::trace!("Load-after-store forwarding at offset {}", static_offset);

                            return match stored.value_type {
                                Type::Int(BitWidth::I256) => Expression::Var(stored.id),
                                Type::Int(width) if width < BitWidth::I256 => {
                                    Expression::ZeroExtend {
                                        value: stored,
                                        to: BitWidth::I256,
                                    }
                                }
                                _ => Expression::Var(stored.id),
                            };
                        }
                    }
                } else {
                    self.pending_stores.clear();
                }
                Expression::MLoad { offset, region }
            }

            Expression::Keccak256 { offset, length } => self.try_fuse_keccak256(offset, length),

            Expression::Keccak256Pair { word0, word1 } => {
                self.pending_stores.clear();
                Expression::Keccak256Pair { word0, word1 }
            }

            Expression::Keccak256Single { word0 } => {
                self.pending_stores.clear();
                Expression::Keccak256Single { word0 }
            }

            Expression::MappingSLoad { key, slot } => {
                self.pending_stores.clear();
                Expression::MappingSLoad { key, slot }
            }

            other => other,
        }
    }

    /// Fuses `keccak256(0, 0x40)` / `keccak256(0, 0x20)` into a `Keccak256Pair` / `Keccak256Single`
    /// node when the scratch words are tracked constants, dead-eliminating the staging `mstore`s.
    /// Falls back to the original `Keccak256` when the pattern does not match.
    ///
    /// Soundness: the `Keccak256Pair`/`Keccak256Single` helpers write their inputs back to
    /// `heap[0..0x40)` / `[0..0x20)` (see `Keccak256OneWord::emit_body` /
    /// `Keccak256TwoWords::emit_body`), so the post-call heap state matches what EVM's
    /// `mstore(...); keccak256(...)` would have produced. That is why the prior `mstore`s can be
    /// dead-eliminated here — a later `mload`, even one mem_opt's forwarding cannot reach (an
    /// intervening clearing event), reads the helper's write-back and sees the value EVM would.
    ///
    /// Caveat: that write-back is lost if the fused node is later constant-folded to a literal (the
    /// helper disappears). That is a deliberate, solc-unreachable gap — see the `fold_constant_keccak`
    /// doc comment in `simplify`.
    fn try_fuse_keccak256(&mut self, offset: Value, length: Value) -> Expression {
        let static_offset = self.try_get_static_offset(&offset);
        let static_length = self.try_get_static_offset(&length);

        if static_offset == Some(0) && static_length == Some(2 * BYTE_LENGTH_WORD as u64) {
            if let (Some(tracked0), Some(tracked32)) = (
                self.memory_state.get(&0),
                self.memory_state.get(&(BYTE_LENGTH_WORD as u64)),
            ) {
                if tracked0.offset == 0 && tracked32.offset == BYTE_LENGTH_WORD as u64 {
                    let word0 = tracked0.stored_value;
                    let word1 = tracked32.stored_value;
                    if let Some(&idx0) = self.pending_stores.get(&0) {
                        self.dead_store_indices.insert(idx0);
                        self.statistics.stores_eliminated += 1;
                    }
                    if let Some(&idx32) = self.pending_stores.get(&(BYTE_LENGTH_WORD as u64)) {
                        self.dead_store_indices.insert(idx32);
                        self.statistics.stores_eliminated += 1;
                    }
                    self.statistics.keccak_pairs_fused += 1;
                    self.pending_stores.clear();
                    log::trace!("Fused keccak256(0, 64) into keccak256_pair");
                    return Expression::Keccak256Pair { word0, word1 };
                }
            }
        }

        if static_offset == Some(0) && static_length == Some(BYTE_LENGTH_WORD as u64) {
            if let Some(tracked0) = self.memory_state.get(&0) {
                if tracked0.offset == 0 {
                    let word0 = tracked0.stored_value;
                    if let Some(&idx0) = self.pending_stores.get(&0) {
                        self.dead_store_indices.insert(idx0);
                        self.statistics.stores_eliminated += 1;
                    }
                    self.statistics.keccak_singles_fused += 1;
                    self.pending_stores.clear();
                    log::trace!("Fused keccak256(0, 32) into keccak256_single");
                    return Expression::Keccak256Single { word0 };
                }
            }
        }

        self.pending_stores.clear();
        Expression::Keccak256 { offset, length }
    }

    /// Intersects two memory states: keeps entries present in both with the same stored value ID.
    fn intersect_memory_state(
        a: &BTreeMap<u64, TrackedValue>,
        b: &BTreeMap<u64, TrackedValue>,
    ) -> BTreeMap<u64, TrackedValue> {
        let mut result = BTreeMap::new();
        for (offset, val_a) in a {
            if let Some(val_b) = b.get(offset) {
                if val_a.stored_value.id == val_b.stored_value.id && val_a.offset == val_b.offset {
                    result.insert(*offset, val_a.clone());
                }
            }
        }
        result
    }

    /// Intersects two constant value maps: keeps entries present in both with the same value.
    fn intersect_constants(
        a: &BTreeMap<u32, BigUint>,
        b: &BTreeMap<u32, BigUint>,
    ) -> BTreeMap<u32, BigUint> {
        let mut result = BTreeMap::new();
        for (id, val_a) in a {
            if let Some(val_b) = b.get(id) {
                if val_a == val_b {
                    result.insert(*id, val_a.clone());
                }
            }
        }
        result
    }

    /// Intersects multiple memory states (for switch with many cases).
    fn intersect_memory_states(
        states: &[BTreeMap<u64, TrackedValue>],
    ) -> BTreeMap<u64, TrackedValue> {
        if states.is_empty() {
            return BTreeMap::new();
        }
        let mut result = states[0].clone();
        for state in &states[1..] {
            result = Self::intersect_memory_state(&result, state);
        }
        result
    }

    /// Intersects multiple constant maps (for switch with many cases).
    fn intersect_all_constants(constants: &[BTreeMap<u32, BigUint>]) -> BTreeMap<u32, BigUint> {
        if constants.is_empty() {
            return BTreeMap::new();
        }
        let mut result = constants[0].clone();
        for c in &constants[1..] {
            result = Self::intersect_constants(&result, c);
        }
        result
    }

    /// Tries to extract a static offset from a Value.
    /// Looks up the value ID in the constant_values map.
    fn try_get_static_offset(&self, value: &Value) -> Option<u64> {
        self.constant_values.get(&value.id.0).and_then(|big| {
            let digits = big.to_u64_digits();
            if digits.is_empty() {
                Some(0)
            } else if digits.len() == 1 {
                Some(digits[0])
            } else {
                None
            }
        })
    }

    /// Tries to evaluate an expression to a constant value.
    fn try_eval_const(&self, expression: &Expression) -> Option<BigUint> {
        match expression {
            Expression::Literal { value, .. } => Some(value.clone()),
            Expression::Var(id) => self.constant_values.get(&id.0).cloned(),
            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let l = self.constant_values.get(&lhs.id.0)?;
                let r = self.constant_values.get(&rhs.id.0)?;
                match operation {
                    BinaryOperation::Add => Some(l + r),
                    BinaryOperation::Sub => {
                        if l >= r {
                            Some(l - r)
                        } else {
                            None
                        }
                    }
                    BinaryOperation::Mul => Some(l * r),
                    BinaryOperation::Div => {
                        if r.is_zero() {
                            None
                        } else {
                            Some(l / r)
                        }
                    }
                    BinaryOperation::And => Some(l & r),
                    BinaryOperation::Or => Some(l | r),
                    BinaryOperation::Xor => Some(l ^ r),
                    BinaryOperation::Shl => {
                        let shift = r.to_u32_digits().first().copied().unwrap_or(0);
                        if shift < 256 {
                            Some(l << shift as usize)
                        } else {
                            Some(BigUint::from(0u32))
                        }
                    }
                    BinaryOperation::Shr => {
                        let shift = r.to_u32_digits().first().copied().unwrap_or(0);
                        if shift < 256 {
                            Some(l >> shift as usize)
                        } else {
                            Some(BigUint::from(0u32))
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Records a constant binding if the expression is constant.
    fn record_constant(&mut self, binding_id: ValueId, expression: &Expression) {
        if let Some(value) = self.try_eval_const(expression) {
            self.constant_values.insert(binding_id.0, value);
        }
    }
}

impl Default for MemoryOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Free memory pointer (FMP) propagation pass.
///
/// Replaces `mload(0x40)` with the known FMP value when provably unchanged.
/// This eliminates expensive heap loads (`__revive_load_heap_word(64)` with sbrk + bswap)
/// in favor of cheap constant materialization.
///
/// In Solidity-generated Yul, the runtime code starts with `mstore(0x40, 0x80)` and
/// most switch cases never update FMP. This pass detects those cases and propagates
/// the constant 0x80 through them.
pub struct FmpPropagation {
    /// Number of FMP loads eliminated.
    pub loads_eliminated: usize,
    /// Set of function IDs that may write to FMP (directly or transitively).
    fmp_writers: BTreeSet<FunctionId>,
}

impl Default for FmpPropagation {
    fn default() -> Self {
        Self::new()
    }
}

impl FmpPropagation {
    /// Creates a new FMP propagation pass.
    pub fn new() -> Self {
        FmpPropagation {
            loads_eliminated: 0,
            fmp_writers: BTreeSet::new(),
        }
    }

    /// Runs FMP propagation on an object.
    pub fn propagate_object(&mut self, object: &mut Object) {
        self.fmp_writers = Self::find_fmp_writers(object);

        self.propagate_block(&mut object.code);
        for function in object.functions.values_mut() {
            self.propagate_block(&mut function.body);
        }
    }

    /// Finds all functions that may write to FMP, including transitive callers.
    fn find_fmp_writers(object: &Object) -> BTreeSet<FunctionId> {
        let mut direct_writers = BTreeSet::new();
        let mut callers: BTreeMap<FunctionId, Vec<FunctionId>> = BTreeMap::new();

        for (fid, function) in &object.functions {
            if Self::statements_write_fmp(&function.body.statements) {
                direct_writers.insert(*fid);
            }
            Self::collect_callees(&function.body.statements, &mut |callee| {
                callers.entry(callee).or_default().push(*fid);
            });
        }

        Self::collect_callees(&object.code.statements, &mut |_callee| {});

        let mut writers = direct_writers.clone();
        let mut worklist: Vec<FunctionId> = direct_writers.into_iter().collect();
        while let Some(fid) = worklist.pop() {
            if let Some(caller_list) = callers.get(&fid) {
                for caller in caller_list {
                    if writers.insert(*caller) {
                        worklist.push(*caller);
                    }
                }
            }
        }

        writers
    }

    /// Collects all function IDs called in a statement list, recursing through
    /// nested regions and `For::condition_statements`.
    fn collect_callees(statements: &[Statement], cb: &mut dyn FnMut(FunctionId)) {
        for_each_statement(statements, &mut |statement| {
            if let Statement::Let { value, .. } | Statement::Expression(value) = statement {
                if let Expression::Call { function, .. } = value {
                    cb(*function);
                }
            }
        });
    }

    /// Propagates FMP through a block.
    fn propagate_block(&mut self, block: &mut Block) {
        let statements = std::mem::take(&mut block.statements);
        block.statements = self.propagate_statements(statements, None);
    }

    /// Propagates FMP through a region.
    fn propagate_region(&mut self, region: &mut Region, fmp_value: Option<BigUint>) {
        let statements = std::mem::take(&mut region.statements);
        region.statements = self.propagate_statements(statements, fmp_value);
    }

    /// Propagates FMP value through a statement list.
    ///
    /// Tracks constant values to resolve MStore/MLoad offsets, and maintains the
    /// known FMP value across control flow when provably safe.
    ///
    /// A dynamic-offset `mstore` whose offset can't be resolved statically could land
    /// on 0x40 at runtime (e.g. `mstore(calldataload(p), v)`). The simplifier leaves
    /// `region == Unknown` for any non-literal offset, so the only safe move is to
    /// invalidate the tracked FMP. `Dynamic` (offset proven `>= 0x80`) and `Scratch`
    /// (`< 0x40`) regions can't reach 0x40 even at runtime, so they are kept.
    fn propagate_statements(
        &mut self,
        statements: Vec<Statement>,
        initial_fmp: Option<BigUint>,
    ) -> Vec<Statement> {
        let mut fmp_value = initial_fmp;
        let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
        let mut result = Vec::with_capacity(statements.len());

        for statement in statements {
            match statement {
                Statement::MStore {
                    offset,
                    value,
                    region,
                } => {
                    let resolved_offset = Self::resolve_offset(&constants, &offset);
                    let is_fmp_store = region.is_free_pointer_slot(resolved_offset);

                    if is_fmp_store {
                        let new_fmp = Self::resolve_value(&constants, &value);
                        fmp_value = new_fmp;
                    } else if word_store_overlaps_free_pointer_slot(resolved_offset)
                        || (resolved_offset.is_none()
                            && region != MemoryRegion::Dynamic
                            && region != MemoryRegion::Scratch)
                    {
                        fmp_value = None;
                    }

                    result.push(Statement::MStore {
                        offset,
                        value,
                        region,
                    });
                }

                Statement::Let { bindings, value } => {
                    let new_value = if let Expression::MLoad {
                        ref offset,
                        ref region,
                    } = value
                    {
                        let resolved_offset = Self::resolve_offset(&constants, offset);
                        let is_fmp_load = region.is_free_pointer_slot(resolved_offset);

                        if is_fmp_load {
                            if let Some(ref known_fmp) = fmp_value {
                                self.loads_eliminated += 1;
                                Some(Expression::Literal {
                                    value: known_fmp.clone(),
                                    value_type: Type::Int(BitWidth::I256),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let final_value = new_value.unwrap_or(value);

                    if bindings.len() == 1 {
                        if let Some(c) = Self::eval_const(&constants, &final_value) {
                            constants.insert(bindings[0].0, c);
                        }
                    }

                    if let Expression::Call { function, .. } = &final_value {
                        if self.fmp_writers.contains(function) {
                            fmp_value = None;
                        }
                    }

                    result.push(Statement::Let {
                        bindings,
                        value: final_value,
                    });
                }

                Statement::If {
                    condition,
                    inputs,
                    mut then_region,
                    else_region,
                    outputs,
                } => {
                    self.propagate_region(&mut then_region, fmp_value.clone());
                    let then_writes = self.region_modifies_fmp(&then_region.statements);

                    let else_region = if let Some(mut er) = else_region {
                        self.propagate_region(&mut er, fmp_value.clone());
                        let else_writes = self.region_modifies_fmp(&er.statements);
                        if then_writes || else_writes {
                            fmp_value = None;
                        }
                        Some(er)
                    } else {
                        if then_writes {
                            fmp_value = None;
                        }
                        None
                    };

                    result.push(Statement::If {
                        condition,
                        inputs,
                        then_region,
                        else_region,
                        outputs,
                    });
                }

                Statement::Switch {
                    scrutinee,
                    inputs,
                    mut cases,
                    default,
                    outputs,
                } => {
                    let mut any_writes = false;

                    for case in &mut cases {
                        self.propagate_region(&mut case.body, fmp_value.clone());
                        if self.region_modifies_fmp(&case.body.statements) {
                            any_writes = true;
                        }
                    }

                    let default = if let Some(mut d) = default {
                        self.propagate_region(&mut d, fmp_value.clone());
                        if self.region_modifies_fmp(&d.statements) {
                            any_writes = true;
                        }
                        Some(d)
                    } else {
                        None
                    };

                    if any_writes {
                        fmp_value = None;
                    }

                    result.push(Statement::Switch {
                        scrutinee,
                        inputs,
                        cases,
                        default,
                        outputs,
                    });
                }

                Statement::For {
                    initial_values,
                    loop_variables,
                    mut condition_statements,
                    condition,
                    mut body,
                    post_input_variables,
                    mut post,
                    outputs,
                } => {
                    let loop_modifies_fmp = self.region_modifies_fmp(&body.statements)
                        || self.region_modifies_fmp(&condition_statements)
                        || self.region_modifies_fmp(&post.statements);
                    let loop_entry_fmp = if loop_modifies_fmp {
                        None
                    } else {
                        fmp_value.clone()
                    };

                    self.propagate_region(&mut body, loop_entry_fmp.clone());
                    condition_statements =
                        self.propagate_statements(condition_statements, loop_entry_fmp.clone());
                    self.propagate_region(&mut post, loop_entry_fmp);

                    if loop_modifies_fmp {
                        fmp_value = None;
                    }

                    result.push(Statement::For {
                        initial_values,
                        loop_variables,
                        condition_statements,
                        condition,
                        body,
                        post_input_variables,
                        post,
                        outputs,
                    });
                }

                Statement::MCopy { dest, src, length } => {
                    if Self::write_may_cover_fmp(&constants, &dest, &length) {
                        fmp_value = None;
                    }
                    result.push(Statement::MCopy { dest, src, length });
                }

                Statement::CallDataCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::CodeCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::ExtCodeCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::ReturnDataCopy {
                    ref dest,
                    ref length,
                    ..
                }
                | Statement::DataCopy {
                    ref dest,
                    ref length,
                    ..
                } => {
                    if Self::write_may_cover_fmp(&constants, dest, length) {
                        fmp_value = None;
                    }
                    result.push(statement);
                }

                Statement::Block(mut region) => {
                    self.propagate_region(&mut region, fmp_value.clone());
                    if self.region_modifies_fmp(&region.statements) {
                        fmp_value = None;
                    }
                    result.push(Statement::Block(region));
                }

                Statement::Expression(ref expression) => {
                    if let Expression::Call { function, .. } = expression {
                        if self.fmp_writers.contains(function) {
                            fmp_value = None;
                        }
                    }
                    result.push(statement);
                }

                Statement::ExternalCall { .. } | Statement::Create { .. } => {
                    fmp_value = None;
                    result.push(statement);
                }

                other => {
                    result.push(other);
                }
            }
        }

        result
    }

    /// Whether a memory write of `length` bytes starting at `dest` could overlap the
    /// free-memory-pointer slot `[0x40, 0x60)`. Conservative: an unresolved destination,
    /// or an unresolved length from a destination below `0x60`, is treated as possibly
    /// covering it. A destination provably `>= 0x60` only writes upward and cannot reach
    /// the slot (so FMP-relative copies to `>= 0x80`, as Solidity emits, never invalidate).
    fn write_may_cover_fmp(
        constants: &BTreeMap<u32, BigUint>,
        dest: &Value,
        length: &Value,
    ) -> bool {
        const FMP_SLOT_START: u64 = 0x40;
        const FMP_SLOT_END: u64 = 0x60;
        match Self::resolve_offset(constants, dest) {
            Some(d) if d >= FMP_SLOT_END => false,
            Some(d) => match Self::resolve_offset(constants, length) {
                Some(l) => d.saturating_add(l) > FMP_SLOT_START,
                None => true,
            },
            None => true,
        }
    }

    /// Checks if any statement (recursively) writes to offset 0x40 or otherwise
    /// invalidates the FMP. The IR translator marks FMP stores explicitly via
    /// `MemoryRegion::FreePointerSlot`; `ExternalCall`/`Create` are conservative
    /// invalidators because they can mutate arbitrary memory. `MCopy` to 0x40
    /// is theoretically possible but is not generated by Solidity in practice
    /// and would defeat the optimization for everyone.
    ///
    /// This is the tag-only direct-write predicate. It deliberately does NOT consult `fmp_writers`
    /// (callers of allocating functions): `find_fmp_writers` uses it to seed that very set, so a
    /// `fmp_writers` lookup here would be circular. Region invalidation must instead use
    /// [`Self::region_modifies_fmp`], which adds the call case.
    fn statements_write_fmp(statements: &[Statement]) -> bool {
        let mut found = false;
        for_each_statement(statements, &mut |statement| {
            if matches!(
                statement,
                Statement::MStore {
                    region: MemoryRegion::FreePointerSlot,
                    ..
                } | Statement::ExternalCall { .. }
                    | Statement::Create { .. }
            ) {
                found = true;
            }
        });
        found
    }

    /// Whether these (recursively walked) statements could change the free-memory pointer: a direct
    /// write to the FMP word, an `ExternalCall`/`Create` that can mutate arbitrary memory, or a call
    /// to a function that (transitively) writes FMP.
    ///
    /// This is the region-level counterpart of the per-statement FMP invalidation that
    /// `propagate_statements` applies to straight-line code, and must match it — in particular the
    /// `fmp_writers` call case, which the tag-only [`Self::statements_write_fmp`] misses. Two uses:
    /// - **Post-region invalidation** for `If`/`Switch`/`Block`: those regions run at most once, so
    ///   a tracked FMP constant survives them only if the region didn't move FMP.
    /// - **For-body gating**: a `For` body re-executes, so a loop that moves FMP must not have the
    ///   pre-loop constant propagated *into* it, or a loop-top `mload(0x40)` would be rewritten to
    ///   iteration 1's pointer and iterations ≥2 would alias that allocation.
    fn region_modifies_fmp(&self, statements: &[Statement]) -> bool {
        let mut found = false;
        for_each_statement(statements, &mut |statement| match statement {
            Statement::MStore {
                region: MemoryRegion::FreePointerSlot,
                ..
            }
            | Statement::ExternalCall { .. }
            | Statement::Create { .. } => found = true,
            Statement::Let {
                value: Expression::Call { function, .. },
                ..
            }
            | Statement::Expression(Expression::Call { function, .. })
                if self.fmp_writers.contains(function) =>
            {
                found = true
            }
            _ => {}
        });
        found
    }

    /// Resolves a Value to a constant offset if known.
    fn resolve_offset(constants: &BTreeMap<u32, BigUint>, value: &Value) -> Option<u64> {
        constants.get(&value.id.0).and_then(|big| {
            let digits = big.to_u64_digits();
            if digits.is_empty() {
                Some(0)
            } else if digits.len() == 1 {
                Some(digits[0])
            } else {
                None
            }
        })
    }

    /// Resolves a Value to a constant BigUint if known.
    fn resolve_value(constants: &BTreeMap<u32, BigUint>, value: &Value) -> Option<BigUint> {
        constants.get(&value.id.0).cloned()
    }

    /// Evaluates an expression to a constant if possible.
    fn eval_const(constants: &BTreeMap<u32, BigUint>, expression: &Expression) -> Option<BigUint> {
        match expression {
            Expression::Literal { value, .. } => Some(value.clone()),
            Expression::Var(id) => constants.get(&id.0).cloned(),
            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let l = constants.get(&lhs.id.0)?;
                let r = constants.get(&rhs.id.0)?;
                match operation {
                    BinaryOperation::Add => Some(l + r),
                    BinaryOperation::Sub => {
                        if l >= r {
                            Some(l - r)
                        } else {
                            None
                        }
                    }
                    BinaryOperation::And => Some(l & r),
                    BinaryOperation::Or => Some(l | r),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BitWidth, Type};

    fn make_value(id: u32) -> Value {
        Value {
            id: ValueId(id),
            value_type: Type::Int(BitWidth::I256),
        }
    }

    #[test]
    fn test_load_after_store_elimination() {
        use crate::ir::{MemoryRegion, Object};

        let mut opt = MemoryOptimizer::new();

        let offset_id = ValueId(1);
        let value_id = ValueId(2);
        let result_id = ValueId(3);

        let statements = vec![
            Statement::Let {
                bindings: vec![offset_id],
                value: Expression::Literal {
                    value: BigUint::from(64u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![value_id],
                value: Expression::Literal {
                    value: BigUint::from(42u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
            Statement::Let {
                bindings: vec![result_id],
                value: Expression::MLoad {
                    offset: Value {
                        id: offset_id,
                        value_type: Type::Int(BitWidth::I256),
                    },
                    region: MemoryRegion::Unknown,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let statistics = opt.optimize_object(&mut object);

        assert_eq!(statistics.loads_eliminated, 1);

        if let Statement::Let { value, .. } = &object.code.statements[3] {
            assert!(
                matches!(value, Expression::Var(_)),
                "Expected Var (forwarded value), got {:?}",
                value
            );
        } else {
            panic!("Expected Let statement");
        }
    }

    #[test]
    fn test_constant_propagation_through_add() {
        use crate::ir::{MemoryRegion, Object};

        let mut opt = MemoryOptimizer::new();

        let base_id = ValueId(1);
        let offset_id = ValueId(2);
        let value_id = ValueId(3);
        let result_id = ValueId(4);

        let statements = vec![
            Statement::Let {
                bindings: vec![base_id],
                value: Expression::Literal {
                    value: BigUint::from(32u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![offset_id],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value {
                        id: base_id,
                        value_type: Type::Int(BitWidth::I256),
                    },
                    rhs: Value {
                        id: ValueId(100),
                        value_type: Type::Int(BitWidth::I256),
                    },
                },
            },
            Statement::Let {
                bindings: vec![value_id],
                value: Expression::Literal {
                    value: BigUint::from(1u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
            Statement::Let {
                bindings: vec![result_id],
                value: Expression::MLoad {
                    offset: Value {
                        id: offset_id,
                        value_type: Type::Int(BitWidth::I256),
                    },
                    region: MemoryRegion::Unknown,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let _stats = opt.optimize_object(&mut object);
    }

    #[test]
    fn test_zero_offset_handling() {
        let mut opt = MemoryOptimizer::new();

        let zero_id = ValueId(1);
        opt.constant_values.insert(zero_id.0, BigUint::from(0u32));

        let value = Value {
            id: zero_id,
            value_type: Type::Int(BitWidth::I256),
        };

        let offset = opt.try_get_static_offset(&value);
        assert_eq!(offset, Some(0));
    }

    #[test]
    fn test_dead_store_elimination() {
        use crate::ir::{MemoryRegion, Object};

        let mut opt = MemoryOptimizer::new();

        let offset_id = ValueId(1);
        let value1_id = ValueId(2);
        let value2_id = ValueId(3);

        let statements = vec![
            Statement::Let {
                bindings: vec![offset_id],
                value: Expression::Literal {
                    value: BigUint::from(64u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![value1_id],
                value: Expression::Literal {
                    value: BigUint::from(1u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![value2_id],
                value: Expression::Literal {
                    value: BigUint::from(2u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value1_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value2_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let statistics = opt.optimize_object(&mut object);

        assert_eq!(statistics.stores_eliminated, 1);

        assert_eq!(object.code.statements.len(), 4);

        if let Statement::MStore { value, .. } = &object.code.statements[3] {
            assert_eq!(value.id, value2_id);
        } else {
            panic!("Expected MStore statement");
        }
    }

    #[test]
    fn test_no_dead_store_when_read() {
        use crate::ir::{MemoryRegion, Object};

        let mut opt = MemoryOptimizer::new();

        let offset_id = ValueId(1);
        let value1_id = ValueId(2);
        let value2_id = ValueId(3);
        let result_id = ValueId(4);

        let statements = vec![
            Statement::Let {
                bindings: vec![offset_id],
                value: Expression::Literal {
                    value: BigUint::from(64u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![value1_id],
                value: Expression::Literal {
                    value: BigUint::from(1u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![value2_id],
                value: Expression::Literal {
                    value: BigUint::from(2u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value1_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
            Statement::Let {
                bindings: vec![result_id],
                value: Expression::MLoad {
                    offset: Value {
                        id: offset_id,
                        value_type: Type::Int(BitWidth::I256),
                    },
                    region: MemoryRegion::Unknown,
                },
            },
            Statement::MStore {
                offset: Value {
                    id: offset_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                value: Value {
                    id: value2_id,
                    value_type: Type::Int(BitWidth::I256),
                },
                region: MemoryRegion::Unknown,
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let statistics = opt.optimize_object(&mut object);

        assert_eq!(statistics.stores_eliminated, 0);

        assert_eq!(statistics.loads_eliminated, 1);

        assert_eq!(object.code.statements.len(), 6);
    }

    #[test]
    fn test_fmp_propagation_basic() {
        use crate::ir::{MemoryRegion, Object};
        use num::BigUint;

        let v1 = make_value(1);
        let v2 = make_value(2);
        let v3_id = ValueId(3);

        let statements = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::Literal {
                    value: BigUint::from(0x80u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Literal {
                    value: BigUint::from(0x40u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: v2,
                value: v1,
                region: MemoryRegion::FreePointerSlot,
            },
            Statement::Let {
                bindings: vec![v3_id],
                value: Expression::MLoad {
                    offset: v2,
                    region: MemoryRegion::FreePointerSlot,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: std::collections::BTreeMap::new(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        };

        let mut fmp = FmpPropagation::new();
        fmp.propagate_object(&mut object);

        assert_eq!(fmp.loads_eliminated, 1, "Should eliminate 1 FMP load");

        if let Statement::Let { value, .. } = &object.code.statements[3] {
            match value {
                Expression::Literal { value: v, .. } => {
                    assert_eq!(*v, BigUint::from(0x80u64), "FMP should be 0x80");
                }
                other => panic!("Expected Literal, got {:?}", other),
            }
        } else {
            panic!("Expected Let statement");
        }
    }

    /// `is_free_pointer_slot` must keep both detection signals: the
    /// `FreePointerSlot` region tag is the only thing that identifies an FMP
    /// access whose offset a pass cannot fold back to a constant.
    ///
    /// Here the offset value (id 9) is never bound to a literal, so
    /// `resolve_offset` returns `None`. Detection relies solely on the region
    /// tag. If that signal were dropped, the unresolved-offset store would
    /// invalidate the tracked FMP and the load would not be eliminated.
    #[test]
    fn test_fmp_eliminated_via_region_tag_with_unresolvable_offset() {
        use crate::ir::{MemoryRegion, Object};
        use num::BigUint;

        let stored_value = make_value(1);
        let unresolvable_offset = make_value(9);
        let load_result = ValueId(3);

        let statements = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::Literal {
                    value: BigUint::from(0x80u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: unresolvable_offset,
                value: stored_value,
                region: MemoryRegion::FreePointerSlot,
            },
            Statement::Let {
                bindings: vec![load_result],
                value: Expression::MLoad {
                    offset: unresolvable_offset,
                    region: MemoryRegion::FreePointerSlot,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: std::collections::BTreeMap::new(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        };

        let mut fmp = FmpPropagation::new();
        fmp.propagate_object(&mut object);

        assert_eq!(
            fmp.loads_eliminated, 1,
            "region tag must detect the FMP load when the offset cannot be resolved"
        );
    }

    /// `is_free_pointer_slot` must keep both detection signals: the
    /// resolved-offset check is the only thing that identifies an FMP access
    /// whose offset was computed (and so could not be classified by
    /// `MemoryRegion::from_address` at translation time).
    ///
    /// Here the offset is computed as `0x20 + 0x20`, folds to `0x40`, and is
    /// tagged `Unknown`. Detection relies solely on the resolved offset. If
    /// that signal were dropped, neither the store nor the load would be
    /// recognised as FMP accesses.
    #[test]
    fn test_fmp_eliminated_via_resolved_offset_when_region_untagged() {
        use crate::ir::{BinaryOperation, MemoryRegion, Object};
        use num::BigUint;

        let stored_value = make_value(1);
        let half_offset = make_value(2);
        let computed_offset = make_value(3);
        let load_result = ValueId(4);

        let statements = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::Literal {
                    value: BigUint::from(0x80u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Literal {
                    value: BigUint::from(0x20u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![ValueId(3)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: half_offset,
                    rhs: half_offset,
                },
            },
            Statement::MStore {
                offset: computed_offset,
                value: stored_value,
                region: MemoryRegion::Unknown,
            },
            Statement::Let {
                bindings: vec![load_result],
                value: Expression::MLoad {
                    offset: computed_offset,
                    region: MemoryRegion::Unknown,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: std::collections::BTreeMap::new(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        };

        let mut fmp = FmpPropagation::new();
        fmp.propagate_object(&mut object);

        assert_eq!(
            fmp.loads_eliminated, 1,
            "resolved offset must detect the FMP load when the region is untagged"
        );
    }

    /// A statically-resolved misaligned store that overlaps the FMP word must invalidate the
    /// tracked free-memory pointer, so a later `mload(0x40)` is not forwarded a stale constant.
    ///
    /// `mstore(0x21, v)` covers bytes `[0x21, 0x41)` and so overwrites byte `0x40` of the FMP
    /// word, but `MemoryRegion::from_address(0x21)` tags it `Scratch` — `is_free_pointer_slot`
    /// alone misses it, and its offset resolves, so the unresolved-offset fallback does not fire
    /// either. Detection relies solely on `word_store_overlaps_free_pointer_slot`; without it the
    /// load would be (wrongly) eliminated to the pre-corruption value.
    #[test]
    fn test_misaligned_overlap_store_invalidates_fmp() {
        use crate::ir::{MemoryRegion, Object};
        use num::BigUint;

        let fmp_initial = make_value(1);
        let fmp_offset = make_value(2);
        let misaligned_offset = make_value(3);
        let stored_value = make_value(1);
        let load_result = ValueId(4);

        let statements = vec![
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::Literal {
                    value: BigUint::from(0x80u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Literal {
                    value: BigUint::from(0x40u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: fmp_offset,
                value: fmp_initial,
                region: MemoryRegion::FreePointerSlot,
            },
            Statement::Let {
                bindings: vec![ValueId(3)],
                value: Expression::Literal {
                    value: BigUint::from(0x21u64),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::MStore {
                offset: misaligned_offset,
                value: stored_value,
                region: MemoryRegion::Scratch,
            },
            Statement::Let {
                bindings: vec![load_result],
                value: Expression::MLoad {
                    offset: fmp_offset,
                    region: MemoryRegion::FreePointerSlot,
                },
            },
        ];

        let mut object = Object {
            name: "test".to_string(),
            code: Block { statements },
            functions: std::collections::BTreeMap::new(),
            subobjects: vec![],
            data: std::collections::BTreeMap::new(),
        };

        let mut fmp = FmpPropagation::new();
        fmp.propagate_object(&mut object);

        assert_eq!(
            fmp.loads_eliminated, 0,
            "a misaligned store overlapping the FMP word must invalidate the tracked pointer"
        );
    }
}
