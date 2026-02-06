//! Function inlining pass for the newyork IR.
//!
//! This module implements custom inlining heuristics tailored for PolkaVM.
//! LLVM's generic inliner lacks domain knowledge about EVM/Solidity patterns,
//! so we make inlining decisions at the IR level where we have more context.
//!
//! # Approach
//!
//! 1. **Call graph analysis**: Count call sites, detect recursion
//! 2. **Heuristic decisions**: Decide which functions to inline based on
//!    call count, function size, and optimization opportunity
//! 3. **IR transformation**: Inline by substituting function bodies at call sites
//! 4. **Dead function removal**: Remove functions that were fully inlined
//!
//! # Inlining Policy
//!
//! - **Always inline**: Functions called exactly once, or with size <= 8 IR nodes
//! - **Never inline**: Recursive functions, functions with size >= 100,
//!   functions called from >= 10 sites
//! - **Cost-benefit**: For everything else, inline if benefit > cost

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{
    BitWidth, Block, Expr, Function, FunctionId, Object, Region, Statement, SwitchCase, Type,
    UnaryOp, Value, ValueId,
};

/// Maximum function size (in IR nodes) that is always inlined regardless of call count.
const ALWAYS_INLINE_SIZE_THRESHOLD: usize = 8;

/// Maximum function size (in IR nodes) beyond which a function is never inlined.
const NEVER_INLINE_SIZE_THRESHOLD: usize = 100;

/// Maximum number of call sites beyond which a function is never inlined.
const NEVER_INLINE_CALL_COUNT_THRESHOLD: usize = 10;

/// Cost multiplier: estimated code size increase per additional call site.
/// When a function is inlined at N call sites, code grows by roughly (N-1) * size.
const CODE_SIZE_COST_MULTIPLIER: usize = 1;

/// Bonus for inlining a small function (enables further optimization).
const SMALL_FUNCTION_BONUS: usize = 15;

/// Results of the call graph analysis.
#[derive(Debug, Clone)]
pub struct CallGraphAnalysis {
    /// Number of call sites per function.
    pub call_counts: BTreeMap<FunctionId, usize>,
    /// Set of functions that are (mutually) recursive.
    pub recursive_functions: BTreeSet<FunctionId>,
    /// Call graph edges: caller -> set of callees.
    pub call_edges: BTreeMap<FunctionId, BTreeSet<FunctionId>>,
    /// Calls from top-level code (not inside any function).
    pub top_level_calls: BTreeSet<FunctionId>,
}

/// Results of the inlining pass.
#[derive(Debug, Clone, Default)]
pub struct InlineResults {
    /// Number of call sites that were inlined.
    pub inlined_call_sites: usize,
    /// Functions that were fully inlined and removed.
    pub removed_functions: BTreeSet<FunctionId>,
    /// Inlining decisions made per function.
    pub decisions: BTreeMap<FunctionId, InlineDecision>,
}

/// The inlining decision for a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineDecision {
    /// Always inline this function at all call sites.
    AlwaysInline,
    /// Never inline this function.
    NeverInline,
    /// Inline based on cost-benefit at each call site.
    CostBenefit,
}

/// Analyzes the call graph of an IR object.
///
/// Walks all functions and the top-level code block to:
/// - Count how many times each function is called
/// - Build caller->callee edges
/// - Detect recursive functions via Tarjan's SCC
pub fn analyze_call_graph(object: &Object) -> CallGraphAnalysis {
    let mut call_counts: BTreeMap<FunctionId, usize> = BTreeMap::new();
    let mut call_edges: BTreeMap<FunctionId, BTreeSet<FunctionId>> = BTreeMap::new();
    let mut top_level_calls: BTreeSet<FunctionId> = BTreeSet::new();

    // Initialize all functions with zero call counts
    for &func_id in object.functions.keys() {
        call_counts.insert(func_id, 0);
        call_edges.insert(func_id, BTreeSet::new());
    }

    // Count calls from top-level code
    count_calls_in_block(&object.code, &mut call_counts, &mut top_level_calls);

    // Count calls from each function body
    for (&func_id, function) in &object.functions {
        let mut callee_set = BTreeSet::new();
        count_calls_in_block(&function.body, &mut call_counts, &mut callee_set);
        call_edges.insert(func_id, callee_set);
    }

    // Detect recursive functions using Tarjan's SCC algorithm
    let recursive_functions = find_recursive_functions(&call_edges);

    CallGraphAnalysis {
        call_counts,
        recursive_functions,
        call_edges,
        top_level_calls,
    }
}

/// Counts call sites in a block, incrementing call_counts and recording callees.
fn count_calls_in_block(
    block: &Block,
    call_counts: &mut BTreeMap<FunctionId, usize>,
    callees: &mut BTreeSet<FunctionId>,
) {
    for stmt in &block.statements {
        count_calls_in_statement(stmt, call_counts, callees);
    }
}

/// Counts call sites in a statement.
fn count_calls_in_statement(
    stmt: &Statement,
    call_counts: &mut BTreeMap<FunctionId, usize>,
    callees: &mut BTreeSet<FunctionId>,
) {
    match stmt {
        Statement::Let { value, .. } => {
            count_calls_in_expr(value, call_counts, callees);
        }
        Statement::Expr(expr) => {
            count_calls_in_expr(expr, call_counts, callees);
        }
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            count_calls_in_region(then_region, call_counts, callees);
            if let Some(else_region) = else_region {
                count_calls_in_region(else_region, call_counts, callees);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                count_calls_in_region(&case.body, call_counts, callees);
            }
            if let Some(default) = default {
                count_calls_in_region(default, call_counts, callees);
            }
        }
        Statement::For {
            condition_stmts,
            condition,
            body,
            post,
            ..
        } => {
            for cond_stmt in condition_stmts {
                count_calls_in_statement(cond_stmt, call_counts, callees);
            }
            count_calls_in_expr(condition, call_counts, callees);
            count_calls_in_region(body, call_counts, callees);
            count_calls_in_region(post, call_counts, callees);
        }
        Statement::Block(region) => {
            count_calls_in_region(region, call_counts, callees);
        }
        _ => {}
    }
}

/// Counts call sites in a region.
fn count_calls_in_region(
    region: &Region,
    call_counts: &mut BTreeMap<FunctionId, usize>,
    callees: &mut BTreeSet<FunctionId>,
) {
    for stmt in &region.statements {
        count_calls_in_statement(stmt, call_counts, callees);
    }
}

/// Counts call sites in an expression.
fn count_calls_in_expr(
    expr: &Expr,
    call_counts: &mut BTreeMap<FunctionId, usize>,
    callees: &mut BTreeSet<FunctionId>,
) {
    if let Expr::Call { function, .. } = expr {
        *call_counts.entry(*function).or_insert(0) += 1;
        callees.insert(*function);
    }
    // Other expression variants don't contain nested calls
    // (arguments are Value references, not expressions)
}

/// Finds recursive functions using iterative SCC detection.
///
/// A function is recursive if it belongs to an SCC of size > 1,
/// or if it has a self-edge (direct recursion).
fn find_recursive_functions(
    call_edges: &BTreeMap<FunctionId, BTreeSet<FunctionId>>,
) -> BTreeSet<FunctionId> {
    let mut recursive = BTreeSet::new();

    // Check for direct recursion
    for (&func_id, callees) in call_edges {
        if callees.contains(&func_id) {
            recursive.insert(func_id);
        }
    }

    // Check for mutual recursion via reachability
    // For each function, check if there's a path back to itself
    let all_functions: Vec<FunctionId> = call_edges.keys().copied().collect();
    for &start in &all_functions {
        if recursive.contains(&start) {
            continue;
        }
        // BFS/DFS from start's callees to see if we can reach start
        let mut visited = BTreeSet::new();
        let mut stack: Vec<FunctionId> = call_edges
            .get(&start)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();

        while let Some(current) = stack.pop() {
            if current == start {
                recursive.insert(start);
                // Also mark all functions in this cycle
                break;
            }
            if !visited.insert(current) {
                continue;
            }
            if let Some(next_callees) = call_edges.get(&current) {
                for &next in next_callees {
                    if !visited.contains(&next) {
                        stack.push(next);
                    }
                }
            }
        }
    }

    recursive
}

/// Estimated IR node overhead per Leave statement during inline expansion.
/// Each Leave adds: accum assignment(s) + done flag + IsZero + If guard.
const LEAVE_OVERHEAD_PER_SITE: usize = 6;

/// Counts the number of Leave statements in a block (non-recursive into functions).
fn count_leaves(block: &Block) -> usize {
    block.statements.iter().map(count_leaves_in_stmt).sum()
}

fn count_leaves_in_stmt(stmt: &Statement) -> usize {
    match stmt {
        Statement::Leave { .. } => 1,
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            count_leaves_in_region(then_region)
                + else_region.as_ref().map_or(0, count_leaves_in_region)
        }
        Statement::Switch { cases, default, .. } => {
            cases
                .iter()
                .map(|c| count_leaves_in_region(&c.body))
                .sum::<usize>()
                + default.as_ref().map_or(0, count_leaves_in_region)
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            condition_stmts
                .iter()
                .map(count_leaves_in_stmt)
                .sum::<usize>()
                + count_leaves_in_region(body)
                + count_leaves_in_region(post)
        }
        Statement::Block(region) => count_leaves_in_region(region),
        _ => 0,
    }
}

fn count_leaves_in_region(region: &Region) -> usize {
    region.statements.iter().map(count_leaves_in_stmt).sum()
}

/// Decides which functions should be inlined.
pub fn make_inline_decisions(
    object: &Object,
    analysis: &CallGraphAnalysis,
) -> BTreeMap<FunctionId, InlineDecision> {
    let mut decisions = BTreeMap::new();

    for (&func_id, function) in &object.functions {
        let call_count = analysis.call_counts.get(&func_id).copied().unwrap_or(0);
        let size = function.size_estimate;
        let is_recursive = analysis.recursive_functions.contains(&func_id);
        let leave_count = count_leaves(&function.body);

        let decision = if is_recursive {
            InlineDecision::NeverInline
        } else if call_count == 0 {
            // Dead function - don't bother inlining, will be removed
            InlineDecision::NeverInline
        } else if size >= NEVER_INLINE_SIZE_THRESHOLD {
            // Very large functions: never inline regardless of call count
            InlineDecision::NeverInline
        } else if call_count == 1 {
            // Single-call functions: always inline (eliminates function entirely)
            InlineDecision::AlwaysInline
        } else if size <= ALWAYS_INLINE_SIZE_THRESHOLD && leave_count == 0 {
            // Very small functions without Leave: always inline
            InlineDecision::AlwaysInline
        } else if call_count >= NEVER_INLINE_CALL_COUNT_THRESHOLD {
            // Called from too many sites: never inline
            InlineDecision::NeverInline
        } else {
            // Cost-benefit analysis accounting for Leave elimination overhead
            let leave_overhead = leave_count * LEAVE_OVERHEAD_PER_SITE * call_count;
            let cost = (call_count - 1) * size * CODE_SIZE_COST_MULTIPLIER + leave_overhead;
            let mut benefit = 0;

            // Small function bonus (only for Leave-free functions)
            if size <= 20 && leave_count == 0 {
                benefit += SMALL_FUNCTION_BONUS;
            }

            // Bonus for having few call sites (code bloat is manageable)
            if call_count <= 3 && leave_count <= 1 {
                benefit += 10;
            }

            if benefit > cost {
                InlineDecision::AlwaysInline
            } else {
                InlineDecision::CostBenefit
            }
        };

        decisions.insert(func_id, decision);
    }

    decisions
}

/// State for SSA value remapping during inlining.
struct InlineRemapper {
    /// Maps old ValueId -> new ValueId.
    value_map: BTreeMap<u32, ValueId>,
    /// Next fresh value ID to allocate.
    next_value_id: u32,
}

impl InlineRemapper {
    /// Creates a new remapper starting from the given next ID.
    fn new(next_value_id: u32) -> Self {
        InlineRemapper {
            value_map: BTreeMap::new(),
            next_value_id,
        }
    }

    /// Gets or creates a fresh ID for the given old ID.
    fn remap_value_id(&mut self, old: ValueId) -> ValueId {
        if let Some(&new_id) = self.value_map.get(&old.0) {
            new_id
        } else {
            let new_id = ValueId::new(self.next_value_id);
            self.next_value_id += 1;
            self.value_map.insert(old.0, new_id);
            new_id
        }
    }

    /// Remaps a Value (id + type).
    fn remap_value(&mut self, value: &Value) -> Value {
        Value {
            id: self.remap_value_id(value.id),
            ty: value.ty,
        }
    }

    /// Remaps an expression, creating fresh value IDs for all references.
    fn remap_expr(&mut self, expr: &Expr) -> Expr {
        match expr {
            Expr::Literal { value, ty } => Expr::Literal {
                value: value.clone(),
                ty: *ty,
            },
            Expr::Var(id) => Expr::Var(self.remap_value_id(*id)),
            Expr::Binary { op, lhs, rhs } => Expr::Binary {
                op: *op,
                lhs: self.remap_value(lhs),
                rhs: self.remap_value(rhs),
            },
            Expr::Ternary { op, a, b, n } => Expr::Ternary {
                op: *op,
                a: self.remap_value(a),
                b: self.remap_value(b),
                n: self.remap_value(n),
            },
            Expr::Unary { op, operand } => Expr::Unary {
                op: *op,
                operand: self.remap_value(operand),
            },
            Expr::CallDataLoad { offset } => Expr::CallDataLoad {
                offset: self.remap_value(offset),
            },
            Expr::CallValue => Expr::CallValue,
            Expr::Caller => Expr::Caller,
            Expr::Origin => Expr::Origin,
            Expr::CallDataSize => Expr::CallDataSize,
            Expr::CodeSize => Expr::CodeSize,
            Expr::GasPrice => Expr::GasPrice,
            Expr::ExtCodeSize { address } => Expr::ExtCodeSize {
                address: self.remap_value(address),
            },
            Expr::ReturnDataSize => Expr::ReturnDataSize,
            Expr::ExtCodeHash { address } => Expr::ExtCodeHash {
                address: self.remap_value(address),
            },
            Expr::BlockHash { number } => Expr::BlockHash {
                number: self.remap_value(number),
            },
            Expr::Coinbase => Expr::Coinbase,
            Expr::Timestamp => Expr::Timestamp,
            Expr::Number => Expr::Number,
            Expr::Difficulty => Expr::Difficulty,
            Expr::GasLimit => Expr::GasLimit,
            Expr::ChainId => Expr::ChainId,
            Expr::SelfBalance => Expr::SelfBalance,
            Expr::BaseFee => Expr::BaseFee,
            Expr::BlobHash { index } => Expr::BlobHash {
                index: self.remap_value(index),
            },
            Expr::BlobBaseFee => Expr::BlobBaseFee,
            Expr::Gas => Expr::Gas,
            Expr::MSize => Expr::MSize,
            Expr::Address => Expr::Address,
            Expr::Balance { address } => Expr::Balance {
                address: self.remap_value(address),
            },
            Expr::MLoad { offset, region } => Expr::MLoad {
                offset: self.remap_value(offset),
                region: *region,
            },
            Expr::SLoad { key, static_slot } => Expr::SLoad {
                key: self.remap_value(key),
                static_slot: static_slot.clone(),
            },
            Expr::TLoad { key } => Expr::TLoad {
                key: self.remap_value(key),
            },
            Expr::Call { function, args } => Expr::Call {
                function: *function,
                args: args.iter().map(|a| self.remap_value(a)).collect(),
            },
            Expr::Truncate { value, to } => Expr::Truncate {
                value: self.remap_value(value),
                to: *to,
            },
            Expr::ZeroExtend { value, to } => Expr::ZeroExtend {
                value: self.remap_value(value),
                to: *to,
            },
            Expr::SignExtendTo { value, to } => Expr::SignExtendTo {
                value: self.remap_value(value),
                to: *to,
            },
            Expr::Keccak256 { offset, length } => Expr::Keccak256 {
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Expr::DataOffset { id } => Expr::DataOffset { id: id.clone() },
            Expr::DataSize { id } => Expr::DataSize { id: id.clone() },
            Expr::LoadImmutable { key } => Expr::LoadImmutable { key: key.clone() },
            Expr::LinkerSymbol { path } => Expr::LinkerSymbol { path: path.clone() },
        }
    }

    /// Remaps a statement.
    fn remap_statement(&mut self, stmt: &Statement) -> Statement {
        match stmt {
            Statement::Let { bindings, value } => Statement::Let {
                bindings: bindings.iter().map(|b| self.remap_value_id(*b)).collect(),
                value: self.remap_expr(value),
            },
            Statement::MStore {
                offset,
                value,
                region,
            } => Statement::MStore {
                offset: self.remap_value(offset),
                value: self.remap_value(value),
                region: *region,
            },
            Statement::MStore8 {
                offset,
                value,
                region,
            } => Statement::MStore8 {
                offset: self.remap_value(offset),
                value: self.remap_value(value),
                region: *region,
            },
            Statement::MCopy { dest, src, length } => Statement::MCopy {
                dest: self.remap_value(dest),
                src: self.remap_value(src),
                length: self.remap_value(length),
            },
            Statement::SStore {
                key,
                value,
                static_slot,
            } => Statement::SStore {
                key: self.remap_value(key),
                value: self.remap_value(value),
                static_slot: static_slot.clone(),
            },
            Statement::TStore { key, value } => Statement::TStore {
                key: self.remap_value(key),
                value: self.remap_value(value),
            },
            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => Statement::If {
                condition: self.remap_value(condition),
                inputs: inputs.iter().map(|v| self.remap_value(v)).collect(),
                then_region: self.remap_region(then_region),
                else_region: else_region.as_ref().map(|r| self.remap_region(r)),
                outputs: outputs.iter().map(|o| self.remap_value_id(*o)).collect(),
            },
            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => Statement::Switch {
                scrutinee: self.remap_value(scrutinee),
                inputs: inputs.iter().map(|v| self.remap_value(v)).collect(),
                cases: cases
                    .iter()
                    .map(|c| SwitchCase {
                        value: c.value.clone(),
                        body: self.remap_region(&c.body),
                    })
                    .collect(),
                default: default.as_ref().map(|r| self.remap_region(r)),
                outputs: outputs.iter().map(|o| self.remap_value_id(*o)).collect(),
            },
            Statement::For {
                init_values,
                loop_vars,
                condition_stmts,
                condition,
                body,
                post_input_vars,
                post,
                outputs,
            } => Statement::For {
                init_values: init_values.iter().map(|v| self.remap_value(v)).collect(),
                loop_vars: loop_vars.iter().map(|v| self.remap_value_id(*v)).collect(),
                condition_stmts: condition_stmts
                    .iter()
                    .map(|s| self.remap_statement(s))
                    .collect(),
                condition: self.remap_expr(condition),
                body: self.remap_region(body),
                post_input_vars: post_input_vars
                    .iter()
                    .map(|v| self.remap_value_id(*v))
                    .collect(),
                post: self.remap_region(post),
                outputs: outputs.iter().map(|o| self.remap_value_id(*o)).collect(),
            },
            Statement::Break { values } => Statement::Break {
                values: values.iter().map(|v| self.remap_value(v)).collect(),
            },
            Statement::Continue { values } => Statement::Continue {
                values: values.iter().map(|v| self.remap_value(v)).collect(),
            },
            Statement::Leave { return_values } => Statement::Leave {
                return_values: return_values.iter().map(|v| self.remap_value(v)).collect(),
            },
            Statement::Revert { offset, length } => Statement::Revert {
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::Return { offset, length } => Statement::Return {
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::Stop => Statement::Stop,
            Statement::Invalid => Statement::Invalid,
            Statement::SelfDestruct { address } => Statement::SelfDestruct {
                address: self.remap_value(address),
            },
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
            } => Statement::ExternalCall {
                kind: *kind,
                gas: self.remap_value(gas),
                address: self.remap_value(address),
                value: value.as_ref().map(|v| self.remap_value(v)),
                args_offset: self.remap_value(args_offset),
                args_length: self.remap_value(args_length),
                ret_offset: self.remap_value(ret_offset),
                ret_length: self.remap_value(ret_length),
                result: self.remap_value_id(*result),
            },
            Statement::Create {
                kind,
                value,
                offset,
                length,
                salt,
                result,
            } => Statement::Create {
                kind: *kind,
                value: self.remap_value(value),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
                salt: salt.as_ref().map(|v| self.remap_value(v)),
                result: self.remap_value_id(*result),
            },
            Statement::Log {
                offset,
                length,
                topics,
            } => Statement::Log {
                offset: self.remap_value(offset),
                length: self.remap_value(length),
                topics: topics.iter().map(|t| self.remap_value(t)).collect(),
            },
            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => Statement::CodeCopy {
                dest: self.remap_value(dest),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => Statement::ExtCodeCopy {
                address: self.remap_value(address),
                dest: self.remap_value(dest),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => Statement::ReturnDataCopy {
                dest: self.remap_value(dest),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::DataCopy {
                dest,
                offset,
                length,
            } => Statement::DataCopy {
                dest: self.remap_value(dest),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => Statement::CallDataCopy {
                dest: self.remap_value(dest),
                offset: self.remap_value(offset),
                length: self.remap_value(length),
            },
            Statement::Block(region) => Statement::Block(self.remap_region(region)),
            Statement::Expr(expr) => Statement::Expr(self.remap_expr(expr)),
            Statement::SetImmutable { key, value } => Statement::SetImmutable {
                key: key.clone(),
                value: self.remap_value(value),
            },
        }
    }

    /// Remaps a region.
    fn remap_region(&mut self, region: &Region) -> Region {
        Region {
            statements: region
                .statements
                .iter()
                .map(|s| self.remap_statement(s))
                .collect(),
            yields: region.yields.iter().map(|v| self.remap_value(v)).collect(),
        }
    }
}

/// Finds the maximum ValueId used in an object (for allocating fresh IDs).
fn find_max_value_id(object: &Object) -> u32 {
    let mut max_id: u32 = 0;

    fn update_max_from_value(value: &Value, max_id: &mut u32) {
        *max_id = (*max_id).max(value.id.0);
    }

    fn update_max_from_value_id(id: &ValueId, max_id: &mut u32) {
        *max_id = (*max_id).max(id.0);
    }

    fn scan_expr(expr: &Expr, max_id: &mut u32) {
        match expr {
            Expr::Var(id) => update_max_from_value_id(id, max_id),
            Expr::Binary { lhs, rhs, .. } => {
                update_max_from_value(lhs, max_id);
                update_max_from_value(rhs, max_id);
            }
            Expr::Ternary { a, b, n, .. } => {
                update_max_from_value(a, max_id);
                update_max_from_value(b, max_id);
                update_max_from_value(n, max_id);
            }
            Expr::Unary { operand, .. } => update_max_from_value(operand, max_id),
            Expr::CallDataLoad { offset } => update_max_from_value(offset, max_id),
            Expr::ExtCodeSize { address } | Expr::ExtCodeHash { address } => {
                update_max_from_value(address, max_id)
            }
            Expr::BlockHash { number } => update_max_from_value(number, max_id),
            Expr::BlobHash { index } => update_max_from_value(index, max_id),
            Expr::Balance { address } => update_max_from_value(address, max_id),
            Expr::MLoad { offset, .. } => update_max_from_value(offset, max_id),
            Expr::SLoad { key, .. } | Expr::TLoad { key } => update_max_from_value(key, max_id),
            Expr::Call { args, .. } => {
                for arg in args {
                    update_max_from_value(arg, max_id);
                }
            }
            Expr::Truncate { value, .. }
            | Expr::ZeroExtend { value, .. }
            | Expr::SignExtendTo { value, .. } => update_max_from_value(value, max_id),
            Expr::Keccak256 { offset, length } => {
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
            }
            _ => {}
        }
    }

    fn scan_statement(stmt: &Statement, max_id: &mut u32) {
        match stmt {
            Statement::Let { bindings, value } => {
                for b in bindings {
                    update_max_from_value_id(b, max_id);
                }
                scan_expr(value, max_id);
            }
            Statement::MStore { offset, value, .. } | Statement::MStore8 { offset, value, .. } => {
                update_max_from_value(offset, max_id);
                update_max_from_value(value, max_id);
            }
            Statement::MCopy { dest, src, length } => {
                update_max_from_value(dest, max_id);
                update_max_from_value(src, max_id);
                update_max_from_value(length, max_id);
            }
            Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
                update_max_from_value(key, max_id);
                update_max_from_value(value, max_id);
            }
            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                update_max_from_value(condition, max_id);
                for v in inputs {
                    update_max_from_value(v, max_id);
                }
                scan_region(then_region, max_id);
                if let Some(r) = else_region {
                    scan_region(r, max_id);
                }
                for o in outputs {
                    update_max_from_value_id(o, max_id);
                }
            }
            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                update_max_from_value(scrutinee, max_id);
                for v in inputs {
                    update_max_from_value(v, max_id);
                }
                for c in cases {
                    scan_region(&c.body, max_id);
                }
                if let Some(r) = default {
                    scan_region(r, max_id);
                }
                for o in outputs {
                    update_max_from_value_id(o, max_id);
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
                for v in init_values {
                    update_max_from_value(v, max_id);
                }
                for v in loop_vars {
                    update_max_from_value_id(v, max_id);
                }
                for s in condition_stmts {
                    scan_statement(s, max_id);
                }
                scan_expr(condition, max_id);
                scan_region(body, max_id);
                for v in post_input_vars {
                    update_max_from_value_id(v, max_id);
                }
                scan_region(post, max_id);
                for o in outputs {
                    update_max_from_value_id(o, max_id);
                }
            }
            Statement::Leave { return_values } => {
                for v in return_values {
                    update_max_from_value(v, max_id);
                }
            }
            Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
            }
            Statement::SelfDestruct { address } => update_max_from_value(address, max_id),
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
                update_max_from_value(gas, max_id);
                update_max_from_value(address, max_id);
                if let Some(v) = value {
                    update_max_from_value(v, max_id);
                }
                update_max_from_value(args_offset, max_id);
                update_max_from_value(args_length, max_id);
                update_max_from_value(ret_offset, max_id);
                update_max_from_value(ret_length, max_id);
                update_max_from_value_id(result, max_id);
            }
            Statement::Create {
                value,
                offset,
                length,
                salt,
                result,
                ..
            } => {
                update_max_from_value(value, max_id);
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
                if let Some(s) = salt {
                    update_max_from_value(s, max_id);
                }
                update_max_from_value_id(result, max_id);
            }
            Statement::Log {
                offset,
                length,
                topics,
            } => {
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
                for t in topics {
                    update_max_from_value(t, max_id);
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
                update_max_from_value(dest, max_id);
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
            }
            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => {
                update_max_from_value(address, max_id);
                update_max_from_value(dest, max_id);
                update_max_from_value(offset, max_id);
                update_max_from_value(length, max_id);
            }
            Statement::Block(region) => scan_region(region, max_id),
            Statement::Expr(expr) => scan_expr(expr, max_id),
            Statement::SetImmutable { value, .. } => update_max_from_value(value, max_id),
            Statement::Break { values } | Statement::Continue { values } => {
                for v in values {
                    update_max_from_value(v, max_id);
                }
            }
            Statement::Stop | Statement::Invalid => {}
        }
    }

    fn scan_region(region: &Region, max_id: &mut u32) {
        for stmt in &region.statements {
            scan_statement(stmt, max_id);
        }
        for v in &region.yields {
            update_max_from_value(v, max_id);
        }
    }

    fn scan_block(block: &Block, max_id: &mut u32) {
        for stmt in &block.statements {
            scan_statement(stmt, max_id);
        }
    }

    scan_block(&object.code, &mut max_id);
    for function in object.functions.values() {
        for (param_id, _) in &function.params {
            update_max_from_value_id(param_id, &mut max_id);
        }
        for id in &function.return_values_initial {
            update_max_from_value_id(id, &mut max_id);
        }
        for id in &function.return_values {
            update_max_from_value_id(id, &mut max_id);
        }
        scan_block(&function.body, &mut max_id);
    }

    max_id
}

/// Checks whether a function can be inlined.
///
/// A function can be inlined if it doesn't have top-level Break/Continue
/// statements (those would conflict with an outer loop context).
/// Leave statements are handled during inlining by wrapping the body
/// in a single-iteration For loop and converting Leave to Break.
fn can_inline(function: &Function) -> bool {
    fn has_top_level_break_continue(block: &Block) -> bool {
        block.statements.iter().any(check_stmt_for_break_continue)
    }

    fn check_stmt_for_break_continue(stmt: &Statement) -> bool {
        match stmt {
            Statement::Break { .. } | Statement::Continue { .. } => true,
            // Recurse into non-loop control flow (break/continue in a For is fine)
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                region_has_break_continue(then_region)
                    || else_region.as_ref().is_some_and(region_has_break_continue)
            }
            Statement::Switch { cases, default, .. } => {
                cases.iter().any(|c| region_has_break_continue(&c.body))
                    || default.as_ref().is_some_and(region_has_break_continue)
            }
            // Break/Continue inside For are fine - they refer to that For
            Statement::For { .. } => false,
            Statement::Block(region) => region_has_break_continue(region),
            _ => false,
        }
    }

    fn region_has_break_continue(region: &Region) -> bool {
        region.statements.iter().any(check_stmt_for_break_continue)
    }

    !has_top_level_break_continue(&function.body)
}

/// Checks if a function has Leave statements inside a For loop body.
/// Leave inside For is not supported by our IR-level inliner.
fn has_leave_in_for(block: &Block) -> bool {
    fn check_stmt(stmt: &Statement) -> bool {
        match stmt {
            Statement::For {
                body,
                post,
                condition_stmts,
                ..
            } => {
                // Leave inside a For loop is not handled by our inliner
                stmts_have_leave(&body.statements)
                    || stmts_have_leave(&post.statements)
                    || stmts_have_leave(condition_stmts)
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                then_region.statements.iter().any(check_stmt)
                    || else_region
                        .as_ref()
                        .is_some_and(|r| r.statements.iter().any(check_stmt))
            }
            Statement::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|c| c.body.statements.iter().any(check_stmt))
                    || default
                        .as_ref()
                        .is_some_and(|r| r.statements.iter().any(check_stmt))
            }
            Statement::Block(region) => region.statements.iter().any(check_stmt),
            _ => false,
        }
    }
    block.statements.iter().any(check_stmt)
}

/// Checks if a slice of statements contains any Leave at any nesting level.
fn stmts_have_leave(stmts: &[Statement]) -> bool {
    stmts.iter().any(stmt_has_leave_recursive)
}

fn stmt_has_leave_recursive(stmt: &Statement) -> bool {
    match stmt {
        Statement::Leave { .. } => true,
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            stmts_have_leave(&then_region.statements)
                || else_region
                    .as_ref()
                    .is_some_and(|r| stmts_have_leave(&r.statements))
        }
        Statement::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmts_have_leave(&c.body.statements))
                || default
                    .as_ref()
                    .is_some_and(|r| stmts_have_leave(&r.statements))
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            stmts_have_leave(condition_stmts)
                || stmts_have_leave(&body.statements)
                || stmts_have_leave(&post.statements)
        }
        Statement::Block(region) => stmts_have_leave(&region.statements),
        _ => false,
    }
}

/// Allocates a fresh ValueId.
fn fresh_id(next_id: &mut u32) -> ValueId {
    let id = ValueId::new(*next_id);
    *next_id += 1;
    id
}

/// Result of Leave elimination on a statement list.
struct LeaveElimResult {
    stmts: Vec<Statement>,
    accum_ids: Vec<ValueId>,
    done_id: Option<ValueId>,
}

/// Eliminates Leave statements from a list of (already remapped) statements.
///
/// Uses the "exit flag" pattern: when Leave is encountered, return values are
/// stored in accumulator variables and a "done" flag is set. Subsequent statements
/// are guarded by `if !done`, using the If statement's inputs/outputs as phi nodes.
fn eliminate_leaves(
    stmts: &[Statement],
    accum_ids: &[ValueId],
    next_id: &mut u32,
) -> LeaveElimResult {
    let mut result_stmts = Vec::new();
    let mut current_accums = accum_ids.to_vec();
    let mut done_id: Option<ValueId> = None;

    for (idx, stmt) in stmts.iter().enumerate() {
        // Once done flag is set, guard all remaining statements
        if let Some(done) = done_id {
            let remaining = &stmts[idx..];
            if !remaining.is_empty() {
                let guarded = wrap_remaining_in_guard(remaining, &current_accums, done, next_id);
                result_stmts.extend(guarded.stmts);
                return LeaveElimResult {
                    stmts: result_stmts,
                    accum_ids: guarded.accum_ids,
                    done_id: guarded.done_id,
                };
            }
            break;
        }

        match stmt {
            // Direct Leave: copy values to accumulators, skip remaining
            Statement::Leave { return_values } => {
                let new_accums: Vec<ValueId> = return_values
                    .iter()
                    .map(|v| {
                        let id = fresh_id(next_id);
                        result_stmts.push(Statement::Let {
                            bindings: vec![id],
                            value: Expr::Var(v.id),
                        });
                        id
                    })
                    .collect();
                let done = fresh_id(next_id);
                result_stmts.push(Statement::Let {
                    bindings: vec![done],
                    value: Expr::Literal {
                        value: num::BigUint::from(1u32),
                        ty: Type::Int(BitWidth::I256),
                    },
                });
                current_accums = new_accums;
                done_id = Some(done);
                // Remaining statements are dead at this level, but we still
                // need to guard them in case there's more processing
                continue;
            }

            // Statement contains Leave in sub-structure
            _ if stmt_has_leave_recursive(stmt) => {
                let sub = transform_leave_stmt(stmt, &current_accums, next_id);
                result_stmts.extend(sub.stmts);
                current_accums = sub.accum_ids;
                done_id = sub.done_id;
            }

            // Normal statement
            _ => {
                result_stmts.push(stmt.clone());
            }
        }
    }

    LeaveElimResult {
        stmts: result_stmts,
        accum_ids: current_accums,
        done_id,
    }
}

/// Wraps remaining statements in `if !done { ... }` guard.
/// When done=true (Leave was taken), inputs flow to outputs unchanged.
/// When done=false, the then_region executes and its yields flow to outputs.
fn wrap_remaining_in_guard(
    stmts: &[Statement],
    accum_ids: &[ValueId],
    done_id: ValueId,
    next_id: &mut u32,
) -> LeaveElimResult {
    let mut pre_stmts = Vec::new();

    let not_done = fresh_id(next_id);
    pre_stmts.push(Statement::Let {
        bindings: vec![not_done],
        value: Expr::Unary {
            op: UnaryOp::IsZero,
            operand: Value {
                id: done_id,
                ty: Type::Int(BitWidth::I256),
            },
        },
    });

    // Recursively process remaining statements
    let inner = eliminate_leaves(stmts, accum_ids, next_id);

    let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();

    let then_yields: Vec<Value> = inner
        .accum_ids
        .iter()
        .map(|&id| Value {
            id,
            ty: Type::Int(BitWidth::I256),
        })
        .collect();
    let inputs: Vec<Value> = accum_ids
        .iter()
        .map(|&id| Value {
            id,
            ty: Type::Int(BitWidth::I256),
        })
        .collect();

    pre_stmts.push(Statement::If {
        condition: Value {
            id: not_done,
            ty: Type::Int(BitWidth::I256),
        },
        inputs,
        then_region: Region {
            statements: inner.stmts,
            yields: then_yields,
        },
        else_region: None,
        outputs: new_accums.clone(),
    });

    LeaveElimResult {
        stmts: pre_stmts,
        accum_ids: new_accums,
        done_id: Some(done_id),
    }
}

/// Collects all ValueIds defined at the top scope of a statement list.
/// This includes Let bindings, If/Switch outputs, but NOT values inside nested regions.
fn collect_top_scope_defs(stmts: &[Statement]) -> Vec<ValueId> {
    let mut defs = Vec::new();
    for stmt in stmts {
        match stmt {
            Statement::Let { bindings, .. } => {
                defs.extend_from_slice(bindings);
            }
            Statement::If { outputs, .. } | Statement::Switch { outputs, .. } => {
                defs.extend_from_slice(outputs);
            }
            Statement::For { outputs, .. } => {
                defs.extend_from_slice(outputs);
            }
            _ => {}
        }
    }
    defs
}

/// Checks if a ValueId is defined anywhere within a region (recursively).
fn region_defines_value(region: &Region, target: ValueId) -> bool {
    for stmt in &region.statements {
        match stmt {
            Statement::Let { bindings, .. } => {
                if bindings.contains(&target) {
                    return true;
                }
            }
            Statement::If {
                outputs,
                then_region,
                else_region,
                ..
            } => {
                if outputs.contains(&target) {
                    return true;
                }
                if region_defines_value(then_region, target) {
                    return true;
                }
                if let Some(r) = else_region {
                    if region_defines_value(r, target) {
                        return true;
                    }
                }
            }
            Statement::Switch {
                outputs,
                cases,
                default,
                ..
            } => {
                if outputs.contains(&target) {
                    return true;
                }
                for c in cases {
                    if region_defines_value(&c.body, target) {
                        return true;
                    }
                }
                if let Some(d) = default {
                    if region_defines_value(d, target) {
                        return true;
                    }
                }
            }
            Statement::For {
                outputs,
                condition_stmts,
                body,
                post,
                ..
            } => {
                if outputs.contains(&target) {
                    return true;
                }
                for s in condition_stmts {
                    if let Statement::Let { bindings, .. } = s {
                        if bindings.contains(&target) {
                            return true;
                        }
                    }
                }
                if region_defines_value(body, target) {
                    return true;
                }
                if region_defines_value(post, target) {
                    return true;
                }
            }
            Statement::Block(r) => {
                if region_defines_value(r, target) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// After leave elimination, some original yield values may be inside guard Ifs
/// (created by `wrap_remaining_in_guard`). This function promotes those values
/// to be additional outputs of the guard If, so they're accessible at the outer scope.
fn promote_yields_from_guards(
    stmts: &mut [Statement],
    yields: &mut [Value],
    top_defs: &[ValueId],
    next_id: &mut u32,
) {
    for yield_val in yields.iter_mut() {
        if top_defs.contains(&yield_val.id) {
            continue; // Already in scope
        }
        // Find the guard If that contains this value and promote it
        for stmt in stmts.iter_mut() {
            if let Statement::If {
                ref mut then_region,
                ref mut inputs,
                ref mut outputs,
                ..
            } = stmt
            {
                if region_defines_value(then_region, yield_val.id) {
                    // Add the value as an additional yield of the then_region
                    then_region.yields.push(*yield_val);
                    // Add a new output for the promoted value
                    let new_out = fresh_id(next_id);
                    outputs.push(new_out);
                    // Add an input for the false case (when done=true, value is dead)
                    // Use any existing input as placeholder since the value won't be used
                    let placeholder = inputs.first().copied().unwrap_or(*yield_val);
                    inputs.push(placeholder);
                    // Update the yield to reference the new output
                    yield_val.id = new_out;
                    break;
                }
            }
        }
    }
}

/// Transforms a statement that contains Leave in its sub-structure.
fn transform_leave_stmt(
    stmt: &Statement,
    accum_ids: &[ValueId],
    next_id: &mut u32,
) -> LeaveElimResult {
    match stmt {
        Statement::If {
            condition,
            inputs: orig_inputs,
            then_region,
            else_region,
            outputs: orig_outputs,
        } => {
            let mut pre_stmts = Vec::new();

            let done_false_id = fresh_id(next_id);
            pre_stmts.push(Statement::Let {
                bindings: vec![done_false_id],
                value: Expr::Literal {
                    value: num::BigUint::from(0u32),
                    ty: Type::Int(BitWidth::I256),
                },
            });

            // Process then branch
            let then_result = if stmts_have_leave(&then_region.statements) {
                eliminate_leaves(&then_region.statements, accum_ids, next_id)
            } else {
                LeaveElimResult {
                    stmts: then_region.statements.clone(),
                    accum_ids: accum_ids.to_vec(),
                    done_id: None,
                }
            };
            let then_done = then_result.done_id.unwrap_or(done_false_id);

            // Build then yields: original yields first, then accums + done
            let mut then_stmts = then_result.stmts;
            let mut then_yields: Vec<Value> = then_region.yields.clone();

            // Fix: promote any out-of-scope yield values from guard Ifs
            let then_top_defs = collect_top_scope_defs(&then_stmts);
            promote_yields_from_guards(&mut then_stmts, &mut then_yields, &then_top_defs, next_id);

            then_yields.extend(then_result.accum_ids.iter().map(|&id| Value {
                id,
                ty: Type::Int(BitWidth::I256),
            }));
            then_yields.push(Value {
                id: then_done,
                ty: Type::Int(BitWidth::I256),
            });

            // Build outputs: original outputs first, then new accums + done
            let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();
            let new_done = fresh_id(next_id);
            let mut all_outputs: Vec<ValueId> = orig_outputs.clone();
            all_outputs.extend(new_accums.iter());
            all_outputs.push(new_done);

            if let Some(else_r) = else_region {
                // Has explicit else branch
                let else_result = if stmts_have_leave(&else_r.statements) {
                    eliminate_leaves(&else_r.statements, accum_ids, next_id)
                } else {
                    LeaveElimResult {
                        stmts: else_r.statements.clone(),
                        accum_ids: accum_ids.to_vec(),
                        done_id: None,
                    }
                };
                let else_done = else_result.done_id.unwrap_or(done_false_id);

                // Build else yields: original yields first, then accums + done
                let mut else_stmts = else_result.stmts;
                let mut else_yields: Vec<Value> = else_r.yields.clone();

                // Fix: promote any out-of-scope yield values from guard Ifs
                let else_top_defs = collect_top_scope_defs(&else_stmts);
                promote_yields_from_guards(
                    &mut else_stmts,
                    &mut else_yields,
                    &else_top_defs,
                    next_id,
                );

                else_yields.extend(else_result.accum_ids.iter().map(|&id| Value {
                    id,
                    ty: Type::Int(BitWidth::I256),
                }));
                else_yields.push(Value {
                    id: else_done,
                    ty: Type::Int(BitWidth::I256),
                });

                // With else: extend inputs to match outputs length
                let mut inputs: Vec<Value> = orig_inputs.clone();
                inputs.extend(accum_ids.iter().map(|&id| Value {
                    id,
                    ty: Type::Int(BitWidth::I256),
                }));
                inputs.push(Value {
                    id: done_false_id,
                    ty: Type::Int(BitWidth::I256),
                });

                pre_stmts.push(Statement::If {
                    condition: *condition,
                    inputs,
                    then_region: Region {
                        statements: then_stmts,
                        yields: then_yields,
                    },
                    else_region: Some(Region {
                        statements: else_stmts,
                        yields: else_yields,
                    }),
                    outputs: all_outputs,
                });
            } else {
                // No else: inputs serve as fallback yields when condition is false
                let mut inputs: Vec<Value> = orig_inputs.clone();
                inputs.extend(accum_ids.iter().map(|&id| Value {
                    id,
                    ty: Type::Int(BitWidth::I256),
                }));
                inputs.push(Value {
                    id: done_false_id,
                    ty: Type::Int(BitWidth::I256),
                });

                pre_stmts.push(Statement::If {
                    condition: *condition,
                    inputs,
                    then_region: Region {
                        statements: then_stmts,
                        yields: then_yields,
                    },
                    else_region: None,
                    outputs: all_outputs,
                });
            }

            LeaveElimResult {
                stmts: pre_stmts,
                accum_ids: new_accums,
                done_id: Some(new_done),
            }
        }

        Statement::Switch {
            scrutinee,
            inputs: orig_inputs,
            cases,
            default,
            outputs: orig_outputs,
        } => {
            let mut pre_stmts = Vec::new();

            let done_false_id = fresh_id(next_id);
            pre_stmts.push(Statement::Let {
                bindings: vec![done_false_id],
                value: Expr::Literal {
                    value: num::BigUint::from(0u32),
                    ty: Type::Int(BitWidth::I256),
                },
            });

            let new_cases: Vec<SwitchCase> = cases
                .iter()
                .map(|c| {
                    let case_result = if stmts_have_leave(&c.body.statements) {
                        eliminate_leaves(&c.body.statements, accum_ids, next_id)
                    } else {
                        LeaveElimResult {
                            stmts: c.body.statements.clone(),
                            accum_ids: accum_ids.to_vec(),
                            done_id: None,
                        }
                    };
                    let case_done = case_result.done_id.unwrap_or(done_false_id);

                    // Original yields first, then accums + done
                    let mut stmts = case_result.stmts;
                    let mut yields: Vec<Value> = c.body.yields.clone();

                    // Fix: promote any out-of-scope yield values from guard Ifs
                    let top_defs = collect_top_scope_defs(&stmts);
                    promote_yields_from_guards(&mut stmts, &mut yields, &top_defs, next_id);

                    yields.extend(case_result.accum_ids.iter().map(|&id| Value {
                        id,
                        ty: Type::Int(BitWidth::I256),
                    }));
                    yields.push(Value {
                        id: case_done,
                        ty: Type::Int(BitWidth::I256),
                    });
                    SwitchCase {
                        value: c.value.clone(),
                        body: Region {
                            statements: stmts,
                            yields,
                        },
                    }
                })
                .collect();

            let new_default = default.as_ref().map(|d| {
                let def_result = if stmts_have_leave(&d.statements) {
                    eliminate_leaves(&d.statements, accum_ids, next_id)
                } else {
                    LeaveElimResult {
                        stmts: d.statements.clone(),
                        accum_ids: accum_ids.to_vec(),
                        done_id: None,
                    }
                };
                let def_done = def_result.done_id.unwrap_or(done_false_id);

                // Original yields first, then accums + done
                let mut stmts = def_result.stmts;
                let mut yields: Vec<Value> = d.yields.clone();

                // Fix: promote any out-of-scope yield values from guard Ifs
                let top_defs = collect_top_scope_defs(&stmts);
                promote_yields_from_guards(&mut stmts, &mut yields, &top_defs, next_id);

                yields.extend(def_result.accum_ids.iter().map(|&id| Value {
                    id,
                    ty: Type::Int(BitWidth::I256),
                }));
                yields.push(Value {
                    id: def_done,
                    ty: Type::Int(BitWidth::I256),
                });
                Region {
                    statements: stmts,
                    yields,
                }
            });

            // Outputs: original first, then new accums + done
            let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();
            let new_done = fresh_id(next_id);
            let mut all_outputs: Vec<ValueId> = orig_outputs.clone();
            all_outputs.extend(new_accums.iter());
            all_outputs.push(new_done);

            // Inputs: original first, then accum + done fallbacks
            let mut inputs: Vec<Value> = orig_inputs.clone();
            inputs.extend(accum_ids.iter().map(|&id| Value {
                id,
                ty: Type::Int(BitWidth::I256),
            }));
            inputs.push(Value {
                id: done_false_id,
                ty: Type::Int(BitWidth::I256),
            });

            pre_stmts.push(Statement::Switch {
                scrutinee: *scrutinee,
                inputs,
                cases: new_cases,
                default: new_default,
                outputs: all_outputs,
            });

            LeaveElimResult {
                stmts: pre_stmts,
                accum_ids: new_accums,
                done_id: Some(new_done),
            }
        }

        Statement::Block(region) => {
            // Flatten: process block contents at current level
            eliminate_leaves(&region.statements, accum_ids, next_id)
        }

        _ => {
            // Shouldn't reach here; emit as-is as fallback
            LeaveElimResult {
                stmts: vec![stmt.clone()],
                accum_ids: accum_ids.to_vec(),
                done_id: None,
            }
        }
    }
}

/// Performs inlining on an IR object.
///
/// This is the main entry point for the inlining pass. It:
/// 1. Analyzes the call graph
/// 2. Makes inlining decisions
/// 3. Performs the actual inlining transformations
/// 4. Removes fully-inlined dead functions
/// 5. Updates call counts and size estimates
pub fn inline_functions(object: &mut Object) -> InlineResults {
    let mut results = InlineResults::default();

    // Analyze call graph
    let analysis = analyze_call_graph(object);

    // Update call_count on Function structs
    for (&func_id, function) in object.functions.iter_mut() {
        function.call_count = analysis.call_counts.get(&func_id).copied().unwrap_or(0);
    }

    // Make decisions
    let decisions = make_inline_decisions(object, &analysis);
    results.decisions = decisions.clone();

    // Find max value ID for fresh ID allocation
    let mut next_value_id = find_max_value_id(object) + 1;

    // Collect functions to inline (we need to clone them since we'll mutate the object)
    let functions_snapshot: BTreeMap<FunctionId, Function> = object.functions.clone();

    // Determine which functions should actually be inlined at the IR level.
    // Functions with Leave inside For loops are deferred to LLVM's inliner.
    let inlineable: BTreeSet<FunctionId> = decisions
        .iter()
        .filter_map(|(&func_id, &decision)| {
            if decision == InlineDecision::AlwaysInline {
                if let Some(function) = functions_snapshot.get(&func_id) {
                    if can_inline(function) && !has_leave_in_for(&function.body) {
                        return Some(func_id);
                    }
                }
            }
            None
        })
        .collect();

    if inlineable.is_empty() {
        return results;
    }

    // Perform inlining in top-level code
    let new_code_stmts = inline_in_statements(
        &object.code.statements,
        &inlineable,
        &functions_snapshot,
        &mut next_value_id,
        &mut results.inlined_call_sites,
    );
    object.code.statements = new_code_stmts;

    // Perform inlining in function bodies
    let func_ids: Vec<FunctionId> = object.functions.keys().copied().collect();
    for func_id in func_ids {
        // Don't inline within functions that are themselves being inlined everywhere
        // (they'll be removed anyway)
        let function = object.functions.get(&func_id).unwrap().clone();
        let new_body_stmts = inline_in_statements(
            &function.body.statements,
            &inlineable,
            &functions_snapshot,
            &mut next_value_id,
            &mut results.inlined_call_sites,
        );
        if let Some(f) = object.functions.get_mut(&func_id) {
            f.body.statements = new_body_stmts;
        }
    }

    // Re-analyze to find dead functions (call_count dropped to 0)
    let new_analysis = analyze_call_graph(object);
    let mut to_remove = Vec::new();
    for (&func_id, &count) in &new_analysis.call_counts {
        if count == 0 {
            to_remove.push(func_id);
            results.removed_functions.insert(func_id);
        }
    }
    for func_id in &to_remove {
        object.functions.remove(func_id);
    }

    // Update remaining functions with new call counts and size estimates
    for (&func_id, function) in object.functions.iter_mut() {
        function.call_count = new_analysis.call_counts.get(&func_id).copied().unwrap_or(0);
        function.size_estimate = estimate_block_size(&function.body);
    }

    results
}

/// Performs inlining within a list of statements.
/// Returns the new list of statements with call sites replaced by inlined bodies.
fn inline_in_statements(
    statements: &[Statement],
    inlineable: &BTreeSet<FunctionId>,
    functions: &BTreeMap<FunctionId, Function>,
    next_value_id: &mut u32,
    inlined_count: &mut usize,
) -> Vec<Statement> {
    let mut result = Vec::new();

    for stmt in statements {
        match stmt {
            // Let binding with a function call - prime inlining target
            Statement::Let {
                bindings,
                value: Expr::Call { function, args },
            } if inlineable.contains(function) => {
                if let Some(func_def) = functions.get(function) {
                    let inlined = inline_call_with_results(func_def, args, bindings, next_value_id);
                    result.extend(inlined);
                    *inlined_count += 1;
                } else {
                    result.push(stmt.clone());
                }
            }

            // Expression statement with a function call (result discarded)
            Statement::Expr(Expr::Call { function, args }) if inlineable.contains(function) => {
                if let Some(func_def) = functions.get(function) {
                    let inlined = inline_call_void(func_def, args, next_value_id);
                    result.extend(inlined);
                    *inlined_count += 1;
                } else {
                    result.push(stmt.clone());
                }
            }

            // Recurse into control flow
            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                result.push(Statement::If {
                    condition: *condition,
                    inputs: inputs.clone(),
                    then_region: inline_in_region(
                        then_region,
                        inlineable,
                        functions,
                        next_value_id,
                        inlined_count,
                    ),
                    else_region: else_region.as_ref().map(|r| {
                        inline_in_region(r, inlineable, functions, next_value_id, inlined_count)
                    }),
                    outputs: outputs.clone(),
                });
            }

            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                result.push(Statement::Switch {
                    scrutinee: *scrutinee,
                    inputs: inputs.clone(),
                    cases: cases
                        .iter()
                        .map(|c| SwitchCase {
                            value: c.value.clone(),
                            body: inline_in_region(
                                &c.body,
                                inlineable,
                                functions,
                                next_value_id,
                                inlined_count,
                            ),
                        })
                        .collect(),
                    default: default.as_ref().map(|r| {
                        inline_in_region(r, inlineable, functions, next_value_id, inlined_count)
                    }),
                    outputs: outputs.clone(),
                });
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
                let new_cond_stmts = inline_in_statements(
                    condition_stmts,
                    inlineable,
                    functions,
                    next_value_id,
                    inlined_count,
                );
                result.push(Statement::For {
                    init_values: init_values.clone(),
                    loop_vars: loop_vars.clone(),
                    condition_stmts: new_cond_stmts,
                    condition: condition.clone(),
                    body: inline_in_region(
                        body,
                        inlineable,
                        functions,
                        next_value_id,
                        inlined_count,
                    ),
                    post_input_vars: post_input_vars.clone(),
                    post: inline_in_region(
                        post,
                        inlineable,
                        functions,
                        next_value_id,
                        inlined_count,
                    ),
                    outputs: outputs.clone(),
                });
            }

            Statement::Block(region) => {
                result.push(Statement::Block(inline_in_region(
                    region,
                    inlineable,
                    functions,
                    next_value_id,
                    inlined_count,
                )));
            }

            // Everything else: pass through
            other => result.push(other.clone()),
        }
    }

    result
}

/// Performs inlining within a region.
fn inline_in_region(
    region: &Region,
    inlineable: &BTreeSet<FunctionId>,
    functions: &BTreeMap<FunctionId, Function>,
    next_value_id: &mut u32,
    inlined_count: &mut usize,
) -> Region {
    Region {
        statements: inline_in_statements(
            &region.statements,
            inlineable,
            functions,
            next_value_id,
            inlined_count,
        ),
        yields: region.yields.clone(),
    }
}

/// Inlines a function call that produces results (used in Let bindings).
///
/// Handles functions with Leave statements by eliminating them using the
/// "exit flag" pattern with If outputs as phi nodes.
fn inline_call_with_results(
    function: &Function,
    args: &[Value],
    result_bindings: &[ValueId],
    next_value_id: &mut u32,
) -> Vec<Statement> {
    let mut remapper = InlineRemapper::new(*next_value_id);
    let mut stmts = Vec::new();

    // Bind parameters
    for ((param_id, _param_ty), arg) in function.params.iter().zip(args.iter()) {
        let new_param_id = remapper.remap_value_id(*param_id);
        stmts.push(Statement::Let {
            bindings: vec![new_param_id],
            value: Expr::Var(arg.id),
        });
    }

    // Initialize return variables
    for &ret_init_id in &function.return_values_initial {
        let new_ret_id = remapper.remap_value_id(ret_init_id);
        stmts.push(Statement::Let {
            bindings: vec![new_ret_id],
            value: Expr::Literal {
                value: num::BigUint::from(0u32),
                ty: Type::Int(BitWidth::I256),
            },
        });
    }

    // Remap the body
    let remapped_body: Vec<Statement> = function
        .body
        .statements
        .iter()
        .map(|s| remapper.remap_statement(s))
        .collect();
    *next_value_id = remapper.next_value_id;

    // Get remapped accumulator and fallthrough IDs
    let initial_accums: Vec<ValueId> = function
        .return_values_initial
        .iter()
        .filter_map(|id| remapper.value_map.get(&id.0).copied())
        .collect();
    let fallthrough_ids: Vec<ValueId> = function
        .return_values
        .iter()
        .filter_map(|id| remapper.value_map.get(&id.0).copied())
        .collect();

    // Add an explicit Leave at the end for uniform handling.
    // This represents the fall-through return path.
    let mut body_with_leave = remapped_body;
    body_with_leave.push(Statement::Leave {
        return_values: fallthrough_ids
            .iter()
            .map(|&id| Value {
                id,
                ty: Type::Int(BitWidth::I256),
            })
            .collect(),
    });

    // Eliminate all Leave statements
    let elim = eliminate_leaves(&body_with_leave, &initial_accums, next_value_id);
    stmts.extend(elim.stmts);

    // Bind results from final accumulators
    for (result_binding, final_ret) in result_bindings.iter().zip(elim.accum_ids.iter()) {
        stmts.push(Statement::Let {
            bindings: vec![*result_binding],
            value: Expr::Var(*final_ret),
        });
    }

    stmts
}

/// Inlines a function call whose result is discarded.
fn inline_call_void(
    function: &Function,
    args: &[Value],
    next_value_id: &mut u32,
) -> Vec<Statement> {
    let mut remapper = InlineRemapper::new(*next_value_id);
    let mut stmts = Vec::new();

    // Bind parameters
    for ((param_id, _), arg) in function.params.iter().zip(args.iter()) {
        let new_param_id = remapper.remap_value_id(*param_id);
        stmts.push(Statement::Let {
            bindings: vec![new_param_id],
            value: Expr::Var(arg.id),
        });
    }

    // Initialize return variables
    for &ret_init_id in &function.return_values_initial {
        let new_ret_id = remapper.remap_value_id(ret_init_id);
        stmts.push(Statement::Let {
            bindings: vec![new_ret_id],
            value: Expr::Literal {
                value: num::BigUint::from(0u32),
                ty: Type::Int(BitWidth::I256),
            },
        });
    }

    // Remap the body
    let remapped_body: Vec<Statement> = function
        .body
        .statements
        .iter()
        .map(|s| remapper.remap_statement(s))
        .collect();
    *next_value_id = remapper.next_value_id;

    // If body has Leave, handle it (even for void functions, Leave skips remaining code)
    if stmts_have_leave(&remapped_body) {
        let initial_accums: Vec<ValueId> = function
            .return_values_initial
            .iter()
            .filter_map(|id| remapper.value_map.get(&id.0).copied())
            .collect();

        let elim = eliminate_leaves(&remapped_body, &initial_accums, next_value_id);
        stmts.extend(elim.stmts);
    } else {
        stmts.extend(remapped_body);
    }

    stmts
}

/// Estimates the size of a block (same metric used by from_yul.rs).
fn estimate_block_size(block: &Block) -> usize {
    block.statements.iter().map(estimate_statement_size).sum()
}

/// Estimates the size of a statement.
fn estimate_statement_size(stmt: &Statement) -> usize {
    match stmt {
        Statement::Let { .. } => 1,
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => 1,
        Statement::SStore { .. } | Statement::TStore { .. } => 1,
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            1 + estimate_region_size(then_region)
                + else_region.as_ref().map_or(0, estimate_region_size)
        }
        Statement::Switch { cases, default, .. } => {
            1 + cases
                .iter()
                .map(|c| estimate_region_size(&c.body))
                .sum::<usize>()
                + default.as_ref().map_or(0, estimate_region_size)
        }
        Statement::For { body, post, .. } => {
            1 + estimate_region_size(body) + estimate_region_size(post)
        }
        Statement::Block(region) => estimate_region_size(region),
        Statement::Expr(_) => 1,
        _ => 1,
    }
}

/// Estimates the size of a region.
fn estimate_region_size(region: &Region) -> usize {
    region.statements.iter().map(estimate_statement_size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    /// Helper to create a simple function with the given number of statements.
    fn make_simple_function(
        id: u32,
        name: &str,
        num_stmts: usize,
        num_params: usize,
        num_returns: usize,
    ) -> Function {
        let mut function = Function::new(FunctionId::new(id), name.to_string());

        // Add parameters
        let mut next_id = id * 1000;
        for _ in 0..num_params {
            function
                .params
                .push((ValueId::new(next_id), Type::Int(BitWidth::I256)));
            next_id += 1;
        }

        // Add return types and initial return values
        for _ in 0..num_returns {
            function.returns.push(Type::Int(BitWidth::I256));
            function.return_values_initial.push(ValueId::new(next_id));
            next_id += 1;
        }

        // Add statements
        for i in 0..num_stmts {
            function.body.push(Statement::Let {
                bindings: vec![ValueId::new(next_id)],
                value: Expr::Literal {
                    value: BigUint::from(i as u32),
                    ty: Type::Int(BitWidth::I256),
                },
            });
            next_id += 1;
        }

        // Set final return values
        if num_returns > 0 {
            // Last N value IDs are the return values
            let first_ret = next_id - num_stmts as u32;
            for i in 0..num_returns {
                function
                    .return_values
                    .push(ValueId::new(first_ret + i as u32));
            }
        }

        function.size_estimate = num_stmts;
        function
    }

    #[test]
    fn test_call_count_analysis() {
        // Create an object with two functions where f calls g twice
        let mut object = Object::new("test".to_string());

        let g = make_simple_function(1, "g", 3, 0, 1);
        let g_id = g.id;

        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Let {
            bindings: vec![ValueId::new(100)],
            value: Expr::Call {
                function: g_id,
                args: vec![],
            },
        });
        f.body.push(Statement::Let {
            bindings: vec![ValueId::new(101)],
            value: Expr::Call {
                function: g_id,
                args: vec![],
            },
        });
        f.size_estimate = 2;
        let f_id = f.id;

        object.functions.insert(f_id, f);
        object.functions.insert(g_id, g);

        // Call f from top-level
        object.code.push(Statement::Expr(Expr::Call {
            function: f_id,
            args: vec![],
        }));

        let analysis = analyze_call_graph(&object);

        assert_eq!(analysis.call_counts.get(&f_id), Some(&1));
        assert_eq!(analysis.call_counts.get(&g_id), Some(&2));
        assert!(analysis.recursive_functions.is_empty());
        assert!(analysis.top_level_calls.contains(&f_id));
    }

    #[test]
    fn test_recursion_detection() {
        let mut object = Object::new("test".to_string());

        // Direct recursion: f calls itself
        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(0),
            args: vec![],
        }));
        f.size_estimate = 1;
        object.functions.insert(f.id, f);

        let analysis = analyze_call_graph(&object);
        assert!(analysis.recursive_functions.contains(&FunctionId::new(0)));
    }

    #[test]
    fn test_mutual_recursion_detection() {
        let mut object = Object::new("test".to_string());

        // Mutual recursion: f calls g, g calls f
        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(1),
            args: vec![],
        }));
        f.size_estimate = 1;

        let mut g = Function::new(FunctionId::new(1), "g".to_string());
        g.body.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(0),
            args: vec![],
        }));
        g.size_estimate = 1;

        object.functions.insert(f.id, f);
        object.functions.insert(g.id, g);

        let analysis = analyze_call_graph(&object);
        assert!(analysis.recursive_functions.contains(&FunctionId::new(0)));
        assert!(analysis.recursive_functions.contains(&FunctionId::new(1)));
    }

    #[test]
    fn test_inline_single_call_function() {
        let mut object = Object::new("test".to_string());

        // Create function g that returns a literal
        let mut g = Function::new(FunctionId::new(1), "g".to_string());
        let g_ret_init = ValueId::new(1000);
        let g_ret_final = ValueId::new(1001);
        g.returns.push(Type::Int(BitWidth::I256));
        g.return_values_initial.push(g_ret_init);
        g.body.push(Statement::Let {
            bindings: vec![g_ret_final],
            value: Expr::Literal {
                value: BigUint::from(42u32),
                ty: Type::Int(BitWidth::I256),
            },
        });
        g.return_values.push(g_ret_final);
        g.size_estimate = 1;
        g.call_count = 0;

        object.functions.insert(g.id, g);

        // Call g from top-level
        object.code.push(Statement::Let {
            bindings: vec![ValueId::new(99)],
            value: Expr::Call {
                function: FunctionId::new(1),
                args: vec![],
            },
        });

        let results = inline_functions(&mut object);

        // g should have been inlined and removed
        assert_eq!(results.inlined_call_sites, 1);
        assert!(results.removed_functions.contains(&FunctionId::new(1)));
        assert!(!object.functions.contains_key(&FunctionId::new(1)));

        // Top-level code should now have the inlined body instead of a call
        assert!(object.code.statements.len() > 1);
        // Verify no Call expressions remain
        let has_call = object.code.statements.iter().any(|s| {
            matches!(
                s,
                Statement::Let {
                    value: Expr::Call { .. },
                    ..
                }
            )
        });
        assert!(!has_call, "Call should have been inlined");
    }

    #[test]
    fn test_never_inline_recursive() {
        let mut object = Object::new("test".to_string());

        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(0),
            args: vec![],
        }));
        f.size_estimate = 1;
        object.functions.insert(f.id, f);

        object.code.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(0),
            args: vec![],
        }));

        let results = inline_functions(&mut object);

        // Recursive function should not be inlined
        assert_eq!(results.inlined_call_sites, 0);
        assert!(object.functions.contains_key(&FunctionId::new(0)));
    }

    #[test]
    fn test_inline_decisions() {
        let mut object = Object::new("test".to_string());

        // Small function called once -> AlwaysInline
        let small = make_simple_function(0, "small", 3, 0, 0);
        object.functions.insert(small.id, small);

        // Medium function called many times -> NeverInline
        let medium = make_simple_function(1, "medium", 50, 0, 0);
        object.functions.insert(medium.id, medium);

        // Large function -> NeverInline
        let large = make_simple_function(2, "large", 150, 0, 0);
        object.functions.insert(large.id, large);

        // Call small once, medium 15 times
        object.code.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(0),
            args: vec![],
        }));
        for _ in 0..15 {
            object.code.push(Statement::Expr(Expr::Call {
                function: FunctionId::new(1),
                args: vec![],
            }));
        }
        object.code.push(Statement::Expr(Expr::Call {
            function: FunctionId::new(2),
            args: vec![],
        }));

        let analysis = analyze_call_graph(&object);
        let decisions = make_inline_decisions(&object, &analysis);

        assert_eq!(
            decisions.get(&FunctionId::new(0)),
            Some(&InlineDecision::AlwaysInline)
        );
        assert_eq!(
            decisions.get(&FunctionId::new(1)),
            Some(&InlineDecision::NeverInline)
        );
        assert_eq!(
            decisions.get(&FunctionId::new(2)),
            Some(&InlineDecision::NeverInline)
        );
    }
}
