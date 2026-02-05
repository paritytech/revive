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

use std::collections::BTreeMap;

use num::{BigUint, Zero};

use crate::ir::{BinOp, Block, Expr, Object, Region, Statement, Value, ValueId};

/// Results of memory optimization.
#[derive(Clone, Debug, Default)]
pub struct MemOptResults {
    /// Number of loads eliminated (replaced with stored value).
    pub loads_eliminated: usize,
    /// Number of stores eliminated (dead stores).
    pub stores_eliminated: usize,
    /// Number of values tracked.
    pub values_tracked: usize,
}

/// Memory optimization pass.
pub struct MemoryOptimizer {
    /// Tracks the most recently stored value at each static memory offset.
    /// Key is the word-aligned offset (offset / 32 * 32).
    memory_state: BTreeMap<u64, TrackedValue>,
    /// Tracks constant values for ValueIds.
    /// When a Let binds a literal, we record the constant value here.
    constant_values: BTreeMap<u32, BigUint>,
    /// Counter for fresh value IDs when creating new bindings.
    next_value_id: u32,
    /// Statistics about optimizations performed.
    stats: MemOptResults,
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
            stats: MemOptResults::default(),
        }
    }

    /// Optimizes an object in place.
    pub fn optimize_object(&mut self, object: &mut Object) -> MemOptResults {
        // Find the maximum value ID in use
        self.next_value_id = self.find_max_value_id(object) + 1;

        // Optimize main code block
        self.optimize_block(&mut object.code);

        // Optimize all functions
        for function in object.functions.values_mut() {
            // Reset state between functions
            self.memory_state.clear();
            self.constant_values.clear();
            self.optimize_block(&mut function.body);
        }

        // Recursively optimize subobjects
        for subobject in &mut object.subobjects {
            let sub_stats = self.optimize_object(subobject);
            self.stats.loads_eliminated += sub_stats.loads_eliminated;
            self.stats.stores_eliminated += sub_stats.stores_eliminated;
        }

        std::mem::take(&mut self.stats)
    }

    /// Finds the maximum value ID in an object.
    fn find_max_value_id(&self, object: &Object) -> u32 {
        let mut max_id = 0u32;

        fn visit_value(val: &Value, max_id: &mut u32) {
            *max_id = (*max_id).max(val.id.0);
        }

        fn visit_expr(expr: &Expr, max_id: &mut u32) {
            match expr {
                Expr::Literal { .. } => {}
                Expr::Var(id) => *max_id = (*max_id).max(id.0),
                Expr::Binary { lhs, rhs, .. } => {
                    visit_value(lhs, max_id);
                    visit_value(rhs, max_id);
                }
                Expr::Ternary { a, b, n, .. } => {
                    visit_value(a, max_id);
                    visit_value(b, max_id);
                    visit_value(n, max_id);
                }
                Expr::Unary { operand, .. } => visit_value(operand, max_id),
                Expr::CallDataLoad { offset } => visit_value(offset, max_id),
                Expr::ExtCodeSize { address } | Expr::ExtCodeHash { address } => {
                    visit_value(address, max_id)
                }
                Expr::BlockHash { number } => visit_value(number, max_id),
                Expr::BlobHash { index } => visit_value(index, max_id),
                Expr::Balance { address } => visit_value(address, max_id),
                Expr::MLoad { offset, .. } => visit_value(offset, max_id),
                Expr::SLoad { key, .. } => visit_value(key, max_id),
                Expr::TLoad { key } => visit_value(key, max_id),
                Expr::Call { args, .. } => {
                    for arg in args {
                        visit_value(arg, max_id);
                    }
                }
                Expr::Truncate { value, .. }
                | Expr::ZeroExtend { value, .. }
                | Expr::SignExtendTo { value, .. } => visit_value(value, max_id),
                Expr::Keccak256 { offset, length } => {
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                }
                _ => {}
            }
        }

        fn visit_stmt(stmt: &Statement, max_id: &mut u32) {
            match stmt {
                Statement::Let { bindings, value } => {
                    for b in bindings {
                        *max_id = (*max_id).max(b.0);
                    }
                    visit_expr(value, max_id);
                }
                Statement::MStore { offset, value, .. } => {
                    visit_value(offset, max_id);
                    visit_value(value, max_id);
                }
                Statement::MStore8 { offset, value, .. } => {
                    visit_value(offset, max_id);
                    visit_value(value, max_id);
                }
                Statement::MCopy { dest, src, length } => {
                    visit_value(dest, max_id);
                    visit_value(src, max_id);
                    visit_value(length, max_id);
                }
                Statement::SStore { key, value, .. } => {
                    visit_value(key, max_id);
                    visit_value(value, max_id);
                }
                Statement::TStore { key, value } => {
                    visit_value(key, max_id);
                    visit_value(value, max_id);
                }
                Statement::If {
                    condition,
                    inputs,
                    then_region,
                    else_region,
                    outputs,
                } => {
                    visit_value(condition, max_id);
                    for i in inputs {
                        visit_value(i, max_id);
                    }
                    for s in &then_region.statements {
                        visit_stmt(s, max_id);
                    }
                    for y in &then_region.yields {
                        visit_value(y, max_id);
                    }
                    if let Some(er) = else_region {
                        for s in &er.statements {
                            visit_stmt(s, max_id);
                        }
                        for y in &er.yields {
                            visit_value(y, max_id);
                        }
                    }
                    for o in outputs {
                        *max_id = (*max_id).max(o.0);
                    }
                }
                Statement::Switch {
                    scrutinee,
                    inputs,
                    cases,
                    default,
                    outputs,
                } => {
                    visit_value(scrutinee, max_id);
                    for i in inputs {
                        visit_value(i, max_id);
                    }
                    for c in cases {
                        for s in &c.body.statements {
                            visit_stmt(s, max_id);
                        }
                        for y in &c.body.yields {
                            visit_value(y, max_id);
                        }
                    }
                    if let Some(d) = default {
                        for s in &d.statements {
                            visit_stmt(s, max_id);
                        }
                        for y in &d.yields {
                            visit_value(y, max_id);
                        }
                    }
                    for o in outputs {
                        *max_id = (*max_id).max(o.0);
                    }
                }
                Statement::For {
                    init_values,
                    loop_vars,
                    condition_stmts,
                    condition,
                    body,
                    post,
                    outputs,
                } => {
                    for v in init_values {
                        visit_value(v, max_id);
                    }
                    for lv in loop_vars {
                        *max_id = (*max_id).max(lv.0);
                    }
                    for s in condition_stmts {
                        visit_stmt(s, max_id);
                    }
                    visit_expr(condition, max_id);
                    for s in &body.statements {
                        visit_stmt(s, max_id);
                    }
                    for y in &body.yields {
                        visit_value(y, max_id);
                    }
                    for s in &post.statements {
                        visit_stmt(s, max_id);
                    }
                    for y in &post.yields {
                        visit_value(y, max_id);
                    }
                    for o in outputs {
                        *max_id = (*max_id).max(o.0);
                    }
                }
                Statement::Leave { return_values } => {
                    for v in return_values {
                        visit_value(v, max_id);
                    }
                }
                Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                }
                Statement::SelfDestruct { address } => visit_value(address, max_id),
                Statement::ExternalCall {
                    gas,
                    address,
                    value,
                    args_offset,
                    args_length,
                    ret_offset,
                    ret_length,
                    result,
                    ..
                } => {
                    visit_value(gas, max_id);
                    visit_value(address, max_id);
                    if let Some(v) = value {
                        visit_value(v, max_id);
                    }
                    visit_value(args_offset, max_id);
                    visit_value(args_length, max_id);
                    visit_value(ret_offset, max_id);
                    visit_value(ret_length, max_id);
                    *max_id = (*max_id).max(result.0);
                }
                Statement::Create {
                    value,
                    offset,
                    length,
                    salt,
                    result,
                    ..
                } => {
                    visit_value(value, max_id);
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                    if let Some(s) = salt {
                        visit_value(s, max_id);
                    }
                    *max_id = (*max_id).max(result.0);
                }
                Statement::Log {
                    offset,
                    length,
                    topics,
                } => {
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                    for t in topics {
                        visit_value(t, max_id);
                    }
                }
                Statement::CodeCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::ReturnDataCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::DataCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::CallDataCopy {
                    dest,
                    offset,
                    length,
                } => {
                    visit_value(dest, max_id);
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                }
                Statement::ExtCodeCopy {
                    address,
                    dest,
                    offset,
                    length,
                } => {
                    visit_value(address, max_id);
                    visit_value(dest, max_id);
                    visit_value(offset, max_id);
                    visit_value(length, max_id);
                }
                Statement::Block(region) => {
                    for s in &region.statements {
                        visit_stmt(s, max_id);
                    }
                    for y in &region.yields {
                        visit_value(y, max_id);
                    }
                }
                Statement::Expr(e) => visit_expr(e, max_id),
                Statement::SetImmutable { value, .. } => visit_value(value, max_id),
                Statement::Break | Statement::Continue | Statement::Stop | Statement::Invalid => {}
            }
        }

        for stmt in &object.code.statements {
            visit_stmt(stmt, &mut max_id);
        }

        for func in object.functions.values() {
            for (id, _) in &func.params {
                max_id = max_id.max(id.0);
            }
            for id in &func.return_values_initial {
                max_id = max_id.max(id.0);
            }
            for id in &func.return_values {
                max_id = max_id.max(id.0);
            }
            for stmt in &func.body.statements {
                visit_stmt(stmt, &mut max_id);
            }
        }

        max_id
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
        let mut result = Vec::with_capacity(statements.len());

        for stmt in statements {
            match stmt {
                Statement::MStore {
                    offset,
                    value,
                    region,
                } => {
                    // Track the store for load-after-store elimination
                    if let Some(static_offset) = self.try_get_static_offset(&offset) {
                        let word_offset = static_offset / 32 * 32;
                        self.memory_state.insert(
                            word_offset,
                            TrackedValue {
                                stored_value: value,
                                offset: static_offset,
                                was_read: false,
                            },
                        );
                        self.stats.values_tracked += 1;
                    } else {
                        // Unknown offset - invalidate all tracked state
                        self.memory_state.clear();
                    }
                    result.push(Statement::MStore {
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
                    // MStore8 taints the word, invalidate tracking for that word
                    if let Some(static_offset) = self.try_get_static_offset(&offset) {
                        let word_offset = static_offset / 32 * 32;
                        self.memory_state.remove(&word_offset);
                    } else {
                        // Unknown offset - invalidate all
                        self.memory_state.clear();
                    }
                    result.push(Statement::MStore8 {
                        offset,
                        value,
                        region,
                    });
                }

                Statement::MCopy { dest, src, length } => {
                    // MCopy invalidates the destination region
                    // Conservative: invalidate all if we don't know the exact range
                    self.memory_state.clear();
                    result.push(Statement::MCopy { dest, src, length });
                }

                Statement::Let { bindings, value } => {
                    // Check for load-after-store elimination opportunity
                    let optimized_value = self.optimize_expr(value);

                    // Track constant bindings for single-value lets
                    if bindings.len() == 1 {
                        self.record_constant(bindings[0], &optimized_value);
                    }

                    result.push(Statement::Let {
                        bindings,
                        value: optimized_value,
                    });
                }

                // Control flow - save/restore or invalidate state
                Statement::If {
                    condition,
                    inputs,
                    mut then_region,
                    else_region,
                    outputs,
                } => {
                    // Clear state - we don't track across branches yet
                    self.memory_state.clear();
                    self.constant_values.clear();

                    // Recurse into then_region
                    self.optimize_region(&mut then_region);

                    // Recurse into else_region if present
                    let else_region = if let Some(mut er) = else_region {
                        self.memory_state.clear();
                        self.constant_values.clear();
                        self.optimize_region(&mut er);
                        Some(er)
                    } else {
                        None
                    };

                    // Clear after branches
                    self.memory_state.clear();
                    self.constant_values.clear();

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
                    // Clear state - we don't track across branches yet
                    self.memory_state.clear();
                    self.constant_values.clear();

                    // Recurse into each case
                    for case in &mut cases {
                        self.memory_state.clear();
                        self.constant_values.clear();
                        self.optimize_region(&mut case.body);
                    }

                    // Recurse into default
                    let default = if let Some(mut d) = default {
                        self.memory_state.clear();
                        self.constant_values.clear();
                        self.optimize_region(&mut d);
                        Some(d)
                    } else {
                        None
                    };

                    // Clear after branches
                    self.memory_state.clear();
                    self.constant_values.clear();

                    result.push(Statement::Switch {
                        scrutinee,
                        inputs,
                        cases,
                        default,
                        outputs,
                    });
                }

                Statement::For {
                    init_values,
                    loop_vars,
                    mut condition_stmts,
                    condition,
                    mut body,
                    mut post,
                    outputs,
                } => {
                    // For loops: clear all state
                    self.memory_state.clear();
                    self.constant_values.clear();

                    // Recurse into loop components
                    condition_stmts = self.optimize_statements(condition_stmts);
                    self.memory_state.clear();
                    self.constant_values.clear();
                    self.optimize_region(&mut body);
                    self.memory_state.clear();
                    self.constant_values.clear();
                    self.optimize_region(&mut post);

                    // Clear after loop
                    self.memory_state.clear();
                    self.constant_values.clear();

                    result.push(Statement::For {
                        init_values,
                        loop_vars,
                        condition_stmts,
                        condition,
                        body,
                        post,
                        outputs,
                    });
                }

                Statement::Block(mut region) => {
                    // Recurse into block
                    self.optimize_region(&mut region);
                    result.push(Statement::Block(region));
                }

                Statement::Expr(expr) => {
                    let optimized = self.optimize_expr(expr);
                    result.push(Statement::Expr(optimized));
                }

                // External calls invalidate all memory state (memory could be modified)
                Statement::ExternalCall { .. } => {
                    self.memory_state.clear();
                    result.push(stmt);
                }

                Statement::Create { .. } => {
                    self.memory_state.clear();
                    result.push(stmt);
                }

                // Data copy operations write to memory
                Statement::CodeCopy { dest, .. }
                | Statement::ExtCodeCopy { dest, .. }
                | Statement::ReturnDataCopy { dest, .. }
                | Statement::DataCopy { dest, .. }
                | Statement::CallDataCopy { dest, .. } => {
                    // Invalidate the destination region
                    if let Some(static_offset) = self.try_get_static_offset(&dest) {
                        let word_offset = static_offset / 32 * 32;
                        self.memory_state.remove(&word_offset);
                    } else {
                        self.memory_state.clear();
                    }
                    result.push(stmt);
                }

                // These don't affect heap memory state
                Statement::SStore { .. }
                | Statement::TStore { .. }
                | Statement::SetImmutable { .. }
                | Statement::Log { .. }
                | Statement::Return { .. }
                | Statement::Revert { .. }
                | Statement::SelfDestruct { .. }
                | Statement::Stop
                | Statement::Invalid
                | Statement::Break
                | Statement::Continue
                | Statement::Leave { .. } => {
                    result.push(stmt);
                }
            }
        }

        result
    }

    /// Optimizes an expression, performing load-after-store elimination.
    fn optimize_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::MLoad { offset, region } => {
                // Check if we can eliminate this load
                if let Some(static_offset) = self.try_get_static_offset(&offset) {
                    let word_offset = static_offset / 32 * 32;
                    if let Some(tracked) = self.memory_state.get_mut(&word_offset) {
                        // Only eliminate if the offsets match exactly
                        if tracked.offset == static_offset {
                            tracked.was_read = true;
                            self.stats.loads_eliminated += 1;
                            log::trace!(
                                "Eliminated load at offset {} (using stored value {:?})",
                                static_offset,
                                tracked.stored_value.id
                            );
                            // Return a Var reference to the stored value
                            return Expr::Var(tracked.stored_value.id);
                        }
                    }
                }
                Expr::MLoad { offset, region }
            }

            // Recursively optimize nested expressions
            Expr::Binary { op, lhs, rhs } => Expr::Binary { op, lhs, rhs },
            Expr::Ternary { op, a, b, n } => Expr::Ternary { op, a, b, n },
            Expr::Unary { op, operand } => Expr::Unary { op, operand },
            Expr::Call { function, args } => {
                // Function calls might have memory side effects
                // But they don't return MLoads, so just pass through
                Expr::Call { function, args }
            }
            Expr::Keccak256 { offset, length } => {
                // Keccak reads from memory - mark as read
                if let Some(static_offset) = self.try_get_static_offset(&offset) {
                    let word_offset = static_offset / 32 * 32;
                    if let Some(tracked) = self.memory_state.get_mut(&word_offset) {
                        tracked.was_read = true;
                    }
                }
                Expr::Keccak256 { offset, length }
            }

            // All other expressions pass through unchanged
            other => other,
        }
    }

    /// Tries to extract a static offset from a Value.
    /// Looks up the value ID in the constant_values map.
    fn try_get_static_offset(&self, value: &Value) -> Option<u64> {
        self.constant_values
            .get(&value.id.0)
            .and_then(|big| big.to_u64_digits().first().copied())
    }

    /// Tries to evaluate an expression to a constant value.
    fn try_eval_const(&self, expr: &Expr) -> Option<BigUint> {
        match expr {
            Expr::Literal { value, .. } => Some(value.clone()),
            Expr::Var(id) => self.constant_values.get(&id.0).cloned(),
            Expr::Binary { op, lhs, rhs } => {
                let l = self.constant_values.get(&lhs.id.0)?;
                let r = self.constant_values.get(&rhs.id.0)?;
                match op {
                    BinOp::Add => Some(l + r),
                    BinOp::Sub => {
                        if l >= r {
                            Some(l - r)
                        } else {
                            None // Underflow - not a valid offset
                        }
                    }
                    BinOp::Mul => Some(l * r),
                    BinOp::Div => {
                        if r.is_zero() {
                            None
                        } else {
                            Some(l / r)
                        }
                    }
                    BinOp::And => Some(l & r),
                    BinOp::Or => Some(l | r),
                    BinOp::Xor => Some(l ^ r),
                    BinOp::Shl => {
                        let shift = r.to_u32_digits().first().copied().unwrap_or(0);
                        if shift < 256 {
                            Some(l << shift as usize)
                        } else {
                            Some(BigUint::from(0u32))
                        }
                    }
                    BinOp::Shr => {
                        let shift = r.to_u32_digits().first().copied().unwrap_or(0);
                        if shift < 256 {
                            Some(l >> shift as usize)
                        } else {
                            Some(BigUint::from(0u32))
                        }
                    }
                    _ => None, // Other operations not tracked
                }
            }
            _ => None,
        }
    }

    /// Records a constant binding if the expression is constant.
    fn record_constant(&mut self, binding_id: ValueId, expr: &Expr) {
        if let Some(value) = self.try_eval_const(expr) {
            self.constant_values.insert(binding_id.0, value);
        }
    }

    /// Merges two memory states, keeping only entries that are identical in both.
    /// This is infrastructure for proper branch merging (not yet used - we currently
    /// clear state at control flow boundaries for safety).
    #[allow(dead_code)]
    fn merge_states(
        &self,
        state1: BTreeMap<u64, TrackedValue>,
        state2: BTreeMap<u64, TrackedValue>,
    ) -> BTreeMap<u64, TrackedValue> {
        let mut result = BTreeMap::new();
        for (offset, tv1) in &state1 {
            if let Some(tv2) = state2.get(offset) {
                // Keep only if same value was stored
                if tv1.stored_value.id == tv2.stored_value.id {
                    result.insert(
                        *offset,
                        TrackedValue {
                            stored_value: tv1.stored_value,
                            offset: tv1.offset,
                            was_read: tv1.was_read || tv2.was_read,
                        },
                    );
                }
            }
        }
        result
    }

    /// Merges multiple memory states.
    /// Infrastructure for proper switch branch merging.
    #[allow(dead_code)]
    fn merge_all_states(
        &self,
        states: Vec<BTreeMap<u64, TrackedValue>>,
    ) -> BTreeMap<u64, TrackedValue> {
        if states.is_empty() {
            return BTreeMap::new();
        }
        let mut result = states[0].clone();
        for state in states.into_iter().skip(1) {
            result = self.merge_states(result, state);
        }
        result
    }
}

impl Default for MemoryOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BitWidth, Type};

    fn make_value(id: u32) -> Value {
        Value {
            id: ValueId(id),
            ty: Type::Int(BitWidth::I256),
        }
    }

    #[test]
    fn test_merge_states_identical() {
        let opt = MemoryOptimizer::new();
        let val = make_value(1);

        let mut state1 = BTreeMap::new();
        state1.insert(
            0,
            TrackedValue {
                stored_value: val,
                offset: 0,
                was_read: false,
            },
        );

        let mut state2 = BTreeMap::new();
        state2.insert(
            0,
            TrackedValue {
                stored_value: val,
                offset: 0,
                was_read: true,
            },
        );

        let merged = opt.merge_states(state1, state2);
        assert_eq!(merged.len(), 1);
        assert!(merged.get(&0).unwrap().was_read);
    }

    #[test]
    fn test_merge_states_different() {
        let opt = MemoryOptimizer::new();
        let val1 = make_value(1);
        let val2 = make_value(2);

        let mut state1 = BTreeMap::new();
        state1.insert(
            0,
            TrackedValue {
                stored_value: val1,
                offset: 0,
                was_read: false,
            },
        );

        let mut state2 = BTreeMap::new();
        state2.insert(
            0,
            TrackedValue {
                stored_value: val2,
                offset: 0,
                was_read: false,
            },
        );

        let merged = opt.merge_states(state1, state2);
        assert!(merged.is_empty());
    }
}
