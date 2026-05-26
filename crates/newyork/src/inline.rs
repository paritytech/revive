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
//! - **Always inline**: Functions called exactly once with size below
//!   `SINGLE_CALL_INLINE_SIZE_THRESHOLD`, or any function with size below
//!   `ALWAYS_INLINE_SIZE_THRESHOLD` and no `Leave` statements
//! - **Never inline**: Recursive functions, functions with size at or above
//!   `NEVER_INLINE_SIZE_THRESHOLD`, or functions called from
//!   `NEVER_INLINE_CALL_COUNT_THRESHOLD`+ sites
//! - **Cost-benefit**: For everything else, inline if benefit > cost
//!
//! # Pipeline position
//!
//! `inline_functions` is invoked twice:
//!
//! 1. **Early** — at the start of `optimize_object_tree`, before simplify /
//!    mem_opt / compound_outlining / guard_narrow. The IR is full of redundant
//!    `Let` bindings and unfolded arithmetic; the thresholds above were
//!    empirically calibrated to that shape (see `INLINER.md`).
//! 2. **Late** — after the parameter narrowing fixed point, via
//!    `run_late_inline_loop` in `lib.rs`. By then every other pass has
//!    canonicalized the IR; many helper functions have collapsed to a handful
//!    of statements. Refreshing `size_estimate` with [`estimate_function_sizes`]
//!    before the call lets the cost model see the post-simplify shape, so the
//!    same thresholds catch newly tiny wrappers that the early pass left
//!    behind.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{
    for_each_statement, for_each_statement_mut, BitWidth, Block, Expression, Function, FunctionId,
    Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value, ValueId,
};

/// Maximum function size (in IR nodes) that is always inlined regardless of call count.
const ALWAYS_INLINE_SIZE_THRESHOLD: usize = 6;

/// Maximum function size (in IR nodes) beyond which a function is never inlined.
const NEVER_INLINE_SIZE_THRESHOLD: usize = 100;

/// Maximum function size for single-call inlining at IR level.
/// Since single-call functions are eliminated entirely (zero code duplication),
/// a higher threshold is justified. The interprocedural optimizations from
/// inlining (constant propagation, dead code elimination, type narrowing)
/// usually outweigh the register pressure increase for moderate-sized functions.
const SINGLE_CALL_INLINE_SIZE_THRESHOLD: usize = 48;

/// Maximum number of call sites beyond which a function is never inlined.
const NEVER_INLINE_CALL_COUNT_THRESHOLD: usize = 100;

/// Bonus for inlining a small function (enables further optimization).
const SMALL_FUNCTION_BONUS: usize = 28;

/// Size threshold for receiving the small-function bonus in the cost-benefit
/// branch. Tightened from 15 to 10 in iter40 after measurement: at size 11–15
/// the bonus pushes marginal functions over the cost line and the resulting
/// inlines add more lowered bytes than they save (`+1,296` regression on the
/// OZ corpus at threshold 15 vs. 10).
const SMALL_FUNCTION_BONUS_SIZE_THRESHOLD: usize = 10;

/// Maximum call count for which the few-callsites bonus applies. The cost term
/// `(call_count - 1) * size` grows linearly with callers, so 2–3 is the regime
/// where the bonus can plausibly offset it. Calibrated alongside the rest of
/// the cost model in commit 6ea5672c ("Improve the inliner") — see INLINER.md.
const FEW_CALLS_BONUS_CALL_THRESHOLD: usize = 3;

/// Maximum Leave count for the few-callsites bonus to apply. Each Leave becomes
/// an exit-flag + phi chain in the inlined body (see [`eliminate_leaves`]); the
/// `leaves² * 6 * call_count` term in `cost` already discourages many-Leave
/// inlines, so this threshold just keeps single-Leave functions in play.
const FEW_CALLS_BONUS_LEAVE_THRESHOLD: usize = 1;

/// Cost-benefit bonus when a function has both few call sites
/// ([`FEW_CALLS_BONUS_CALL_THRESHOLD`]) and few Leaves
/// ([`FEW_CALLS_BONUS_LEAVE_THRESHOLD`]). Retuned from 10 to 15 in commit
/// 6ea5672c as part of an end-to-end cost-model retune that landed −8.95% on
/// the OZ corpus. INLINER.md records that the model is at a local optimum on
/// this axis, so don't perturb in isolation without re-running the corpus.
const FEW_CALLS_BONUS: usize = 15;

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
    /// Always inline this function at every call site.
    AlwaysInline,
    /// Never inline this function — emitted as a standalone callable.
    NeverInline,
    /// The cost-benefit heuristic could not justify inlining. The IR-level pass
    /// leaves the function intact; downstream codegen also marks multi-call
    /// `CostBenefit` functions with LLVM `NoInline` so the LLVM inliner does
    /// not undo the decision by inlining at every call site (a behavior that
    /// regressed code size on the PolkaVM target — see iter38 in
    /// `RALPH_TASK.md`).
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

    for &func_id in object.functions.keys() {
        call_counts.insert(func_id, 0);
        call_edges.insert(func_id, BTreeSet::new());
    }

    count_calls_in_block(&object.code, &mut call_counts, &mut top_level_calls);

    for (&func_id, function) in &object.functions {
        let mut callee_set = BTreeSet::new();
        count_calls_in_block(&function.body, &mut call_counts, &mut callee_set);
        call_edges.insert(func_id, callee_set);
    }

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
    for_each_statement(&block.statements, &mut |statement| {
        statement.for_each_expression(&mut |expression| {
            if let Expression::Call { function, .. } = expression {
                *call_counts.entry(*function).or_insert(0) += 1;
                callees.insert(*function);
            }
        });
    });
}

/// Finds recursive functions using iterative SCC detection.
///
/// A function is recursive if it belongs to an SCC of size > 1,
/// or if it has a self-edge (direct recursion).
fn find_recursive_functions(
    call_edges: &BTreeMap<FunctionId, BTreeSet<FunctionId>>,
) -> BTreeSet<FunctionId> {
    let mut recursive = BTreeSet::new();

    for (&func_id, callees) in call_edges {
        if callees.contains(&func_id) {
            recursive.insert(func_id);
        }
    }

    let all_functions: Vec<FunctionId> = call_edges.keys().copied().collect();
    for &start in &all_functions {
        if recursive.contains(&start) {
            continue;
        }
        let mut visited = BTreeSet::new();
        let mut stack: Vec<FunctionId> = call_edges
            .get(&start)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();

        while let Some(current) = stack.pop() {
            if current == start {
                recursive.insert(start);
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
/// Each Leave adds: accum assignment(s) + done flag + IsZero + If guard (~6 nodes).
/// Additionally, each Leave wraps all subsequent statements in a nested guard,
/// so the overhead grows quadratically with the number of Leaves (N Leaves
/// produce O(N^2) nesting as each guard re-wraps the remaining guarded code).
const LEAVE_OVERHEAD_PER_SITE: usize = 6;

/// Counts the number of Leave statements in a block (recursing into nested
/// regions and `For::condition_statements`, but not into other functions).
fn count_leaves(block: &Block) -> usize {
    let mut count = 0;
    for_each_statement(&block.statements, &mut |statement| {
        if matches!(statement, Statement::Leave { .. }) {
            count += 1;
        }
    });
    count
}

/// Recomputes the `size_estimate` field on every function from its current body.
///
/// The initial size estimates are recorded by the Yul translator before any simplification has
/// occurred. After simplify/narrow passes change the IR, re-estimation is required so the inliner
/// makes decisions on the post-simplify shape rather than the original pre-simplify shape.
pub fn estimate_function_sizes(object: &mut Object) {
    for function in object.functions.values_mut() {
        function.size_estimate = estimate_block_size(&function.body);
    }
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

        let decision = if is_recursive || call_count == 0 || size >= NEVER_INLINE_SIZE_THRESHOLD {
            InlineDecision::NeverInline
        } else if call_count == 1 && size <= SINGLE_CALL_INLINE_SIZE_THRESHOLD {
            InlineDecision::AlwaysInline
        } else if call_count == 1 {
            InlineDecision::CostBenefit
        } else if size <= ALWAYS_INLINE_SIZE_THRESHOLD && leave_count == 0 {
            InlineDecision::AlwaysInline
        } else if call_count >= NEVER_INLINE_CALL_COUNT_THRESHOLD {
            InlineDecision::NeverInline
        } else {
            let leave_overhead = leave_count * leave_count * LEAVE_OVERHEAD_PER_SITE * call_count;
            let cost = (call_count - 1) * size + leave_overhead;
            let mut benefit = 0;

            if size <= SMALL_FUNCTION_BONUS_SIZE_THRESHOLD && leave_count == 0 {
                benefit += SMALL_FUNCTION_BONUS;
            }

            if call_count <= FEW_CALLS_BONUS_CALL_THRESHOLD
                && leave_count <= FEW_CALLS_BONUS_LEAVE_THRESHOLD
            {
                benefit += FEW_CALLS_BONUS;
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
    value_map: BTreeMap<ValueId, ValueId>,
    /// Next fresh value ID to allocate.
    next_value_id: ValueId,
}

impl InlineRemapper {
    /// Creates a new remapper starting from the given next ID.
    fn new(next_value_id: ValueId) -> Self {
        InlineRemapper {
            value_map: BTreeMap::new(),
            next_value_id,
        }
    }

    /// Gets or creates a fresh ID for the given old ID.
    fn remap_value_id(&mut self, old: ValueId) -> ValueId {
        if let Some(&new_id) = self.value_map.get(&old) {
            new_id
        } else {
            let new_id = self.next_value_id.fresh();
            self.value_map.insert(old, new_id);
            new_id
        }
    }

    /// Clones the source statements and rewrites every `ValueId` (definitions and
    /// uses) through `remap_value_id`. Used to inline a function body at a
    /// call site with a fresh SSA namespace.
    fn remap_statements(&mut self, source: &[Statement]) -> Vec<Statement> {
        let mut statements = source.to_vec();
        for_each_statement_mut(&mut statements, &mut |statement| {
            statement.for_each_value_id_def_mut(&mut |id| *id = self.remap_value_id(*id));
        });
        for statement in &mut statements {
            statement.for_each_value_id_mut(&mut |id| *id = self.remap_value_id(*id));
        }
        statements
    }
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

    fn check_stmt_for_break_continue(statement: &Statement) -> bool {
        match statement {
            Statement::Break { .. } | Statement::Continue { .. } => true,
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

/// Checks if a function has Leave statements inside any For loop (at any
/// nesting level). Leave inside For is not handled by our IR-level inliner.
fn has_leave_in_for(block: &Block) -> bool {
    let mut found = false;
    for_each_statement(&block.statements, &mut |statement| {
        if let Statement::For {
            body,
            post,
            condition_statements,
            ..
        } = statement
        {
            if statements_have_leave(&body.statements)
                || statements_have_leave(&post.statements)
                || statements_have_leave(condition_statements)
            {
                found = true;
            }
        }
    });
    found
}

/// Checks if a slice of statements contains any Leave at any nesting level.
fn statements_have_leave(statements: &[Statement]) -> bool {
    let mut found = false;
    for_each_statement(statements, &mut |s| {
        if matches!(s, Statement::Leave { .. }) {
            found = true;
        }
    });
    found
}

fn statement_has_leave_recursive(statement: &Statement) -> bool {
    statements_have_leave(std::slice::from_ref(statement))
}

/// Allocates a fresh ValueId.
fn fresh_id(next_id: &mut ValueId) -> ValueId {
    next_id.fresh()
}

/// Result of Leave elimination on a statement list.
struct LeaveElimResult {
    statements: Vec<Statement>,
    accum_ids: Vec<ValueId>,
    done_id: Option<ValueId>,
}

/// Eliminates Leave statements from a list of (already remapped) statements.
///
/// Uses the "exit flag" pattern: when Leave is encountered, return values are
/// stored in accumulator variables and a "done" flag is set. Subsequent statements
/// are guarded by `if !done`, using the If statement's inputs/outputs as phi nodes.
fn eliminate_leaves(
    statements: &[Statement],
    accum_ids: &[ValueId],
    next_id: &mut ValueId,
) -> LeaveElimResult {
    let mut result_statements = Vec::new();
    let mut current_accums = accum_ids.to_vec();
    let mut done_id: Option<ValueId> = None;

    for (index, statement) in statements.iter().enumerate() {
        if let Some(done) = done_id {
            let remaining = &statements[index..];
            if !remaining.is_empty() {
                let guarded = wrap_remaining_in_guard(remaining, &current_accums, done, next_id);
                result_statements.extend(guarded.statements);
                return LeaveElimResult {
                    statements: result_statements,
                    accum_ids: guarded.accum_ids,
                    done_id: guarded.done_id,
                };
            }
            break;
        }

        match statement {
            Statement::Leave { return_values } => {
                let new_accums: Vec<ValueId> = return_values
                    .iter()
                    .map(|v| {
                        let id = fresh_id(next_id);
                        result_statements.push(Statement::Let {
                            bindings: vec![id],
                            value: Expression::Var(v.id),
                        });
                        id
                    })
                    .collect();
                let done = fresh_id(next_id);
                result_statements.push(Statement::Let {
                    bindings: vec![done],
                    value: Expression::Literal {
                        value: num::BigUint::from(1u32),
                        value_type: Type::Int(BitWidth::I256),
                    },
                });
                current_accums = new_accums;
                done_id = Some(done);
                continue;
            }

            _ if statement_has_leave_recursive(statement) => {
                let transformed = transform_leave_statement(statement, &current_accums, next_id);
                result_statements.extend(transformed.statements);
                current_accums = transformed.accum_ids;
                done_id = transformed.done_id;
            }

            _ => {
                result_statements.push(statement.clone());
            }
        }
    }

    LeaveElimResult {
        statements: result_statements,
        accum_ids: current_accums,
        done_id,
    }
}

/// Wraps remaining statements in `if !done { ... }` guard.
/// When done=true (Leave was taken), inputs flow to outputs unchanged.
/// When done=false, the then_region executes and its yields flow to outputs.
fn wrap_remaining_in_guard(
    statements: &[Statement],
    accum_ids: &[ValueId],
    done_id: ValueId,
    next_id: &mut ValueId,
) -> LeaveElimResult {
    let mut pre_stmts = Vec::new();

    let not_done = fresh_id(next_id);
    pre_stmts.push(Statement::Let {
        bindings: vec![not_done],
        value: Expression::Unary {
            operation: UnaryOperation::IsZero,
            operand: Value {
                id: done_id,
                value_type: Type::Int(BitWidth::I256),
            },
        },
    });

    let inner = eliminate_leaves(statements, accum_ids, next_id);

    let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();

    let then_yields: Vec<Value> = inner
        .accum_ids
        .iter()
        .map(|&id| Value {
            id,
            value_type: Type::Int(BitWidth::I256),
        })
        .collect();
    let inputs: Vec<Value> = accum_ids
        .iter()
        .map(|&id| Value {
            id,
            value_type: Type::Int(BitWidth::I256),
        })
        .collect();

    pre_stmts.push(Statement::If {
        condition: Value {
            id: not_done,
            value_type: Type::Int(BitWidth::I256),
        },
        inputs,
        then_region: Region {
            statements: inner.statements,
            yields: then_yields,
        },
        else_region: None,
        outputs: new_accums.clone(),
    });

    LeaveElimResult {
        statements: pre_stmts,
        accum_ids: new_accums,
        done_id: Some(done_id),
    }
}

/// Collects all ValueIds defined at the top scope of a statement list.
/// This includes Let bindings, If/Switch outputs, but NOT values inside nested regions.
fn collect_top_scope_defs(statements: &[Statement]) -> Vec<ValueId> {
    let mut definitions = Vec::new();
    for statement in statements {
        match statement {
            Statement::Let { bindings, .. } => {
                definitions.extend_from_slice(bindings);
            }
            Statement::If { outputs, .. } | Statement::Switch { outputs, .. } => {
                definitions.extend_from_slice(outputs);
            }
            Statement::For { outputs, .. } => {
                definitions.extend_from_slice(outputs);
            }
            _ => {}
        }
    }
    definitions
}

/// Checks if a ValueId is defined anywhere within a region (recursively).
fn region_defines_value(region: &Region, target: ValueId) -> bool {
    for statement in &region.statements {
        match statement {
            Statement::Let { bindings, .. } if bindings.contains(&target) => {
                return true;
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
                condition_statements,
                body,
                post,
                ..
            } => {
                if outputs.contains(&target) {
                    return true;
                }
                for s in condition_statements {
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
            Statement::Block(r) if region_defines_value(r, target) => {
                return true;
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
    statements: &mut [Statement],
    yields: &mut [Value],
    top_defs: &[ValueId],
    next_id: &mut ValueId,
) {
    for yield_val in yields.iter_mut() {
        if top_defs.contains(&yield_val.id) {
            continue;
        }
        for statement in statements.iter_mut() {
            if let Statement::If {
                ref mut then_region,
                ref mut inputs,
                ref mut outputs,
                ..
            } = statement
            {
                if region_defines_value(then_region, yield_val.id) {
                    then_region.yields.push(*yield_val);
                    let new_out = fresh_id(next_id);
                    outputs.push(new_out);
                    let placeholder = inputs.first().copied().unwrap_or(*yield_val);
                    inputs.push(placeholder);
                    yield_val.id = new_out;
                    break;
                }
            }
        }
    }
}

/// Transforms a statement that contains Leave in its sub-structure.
fn transform_leave_statement(
    statement: &Statement,
    accum_ids: &[ValueId],
    next_id: &mut ValueId,
) -> LeaveElimResult {
    match statement {
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
                value: Expression::Literal {
                    value: num::BigUint::from(0u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            });

            let then_result = if statements_have_leave(&then_region.statements) {
                eliminate_leaves(&then_region.statements, accum_ids, next_id)
            } else {
                LeaveElimResult {
                    statements: then_region.statements.clone(),
                    accum_ids: accum_ids.to_vec(),
                    done_id: None,
                }
            };
            let then_done = then_result.done_id.unwrap_or(done_false_id);

            let mut then_stmts = then_result.statements;
            let mut then_yields: Vec<Value> = then_region.yields.clone();

            let then_top_defs = collect_top_scope_defs(&then_stmts);
            promote_yields_from_guards(&mut then_stmts, &mut then_yields, &then_top_defs, next_id);

            then_yields.extend(then_result.accum_ids.iter().map(|&id| Value {
                id,
                value_type: Type::Int(BitWidth::I256),
            }));
            then_yields.push(Value {
                id: then_done,
                value_type: Type::Int(BitWidth::I256),
            });

            let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();
            let new_done = fresh_id(next_id);
            let mut all_outputs: Vec<ValueId> = orig_outputs.clone();
            all_outputs.extend(new_accums.iter());
            all_outputs.push(new_done);

            if let Some(else_r) = else_region {
                let else_result = if statements_have_leave(&else_r.statements) {
                    eliminate_leaves(&else_r.statements, accum_ids, next_id)
                } else {
                    LeaveElimResult {
                        statements: else_r.statements.clone(),
                        accum_ids: accum_ids.to_vec(),
                        done_id: None,
                    }
                };
                let else_done = else_result.done_id.unwrap_or(done_false_id);

                let mut else_stmts = else_result.statements;
                let mut else_yields: Vec<Value> = else_r.yields.clone();

                let else_top_defs = collect_top_scope_defs(&else_stmts);
                promote_yields_from_guards(
                    &mut else_stmts,
                    &mut else_yields,
                    &else_top_defs,
                    next_id,
                );

                else_yields.extend(else_result.accum_ids.iter().map(|&id| Value {
                    id,
                    value_type: Type::Int(BitWidth::I256),
                }));
                else_yields.push(Value {
                    id: else_done,
                    value_type: Type::Int(BitWidth::I256),
                });

                let mut inputs: Vec<Value> = orig_inputs.clone();
                inputs.extend(accum_ids.iter().map(|&id| Value {
                    id,
                    value_type: Type::Int(BitWidth::I256),
                }));
                inputs.push(Value {
                    id: done_false_id,
                    value_type: Type::Int(BitWidth::I256),
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
                let mut inputs: Vec<Value> = orig_inputs.clone();
                inputs.extend(accum_ids.iter().map(|&id| Value {
                    id,
                    value_type: Type::Int(BitWidth::I256),
                }));
                inputs.push(Value {
                    id: done_false_id,
                    value_type: Type::Int(BitWidth::I256),
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
                statements: pre_stmts,
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
                value: Expression::Literal {
                    value: num::BigUint::from(0u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            });

            let new_cases: Vec<SwitchCase> = cases
                .iter()
                .map(|c| {
                    let case_result = if statements_have_leave(&c.body.statements) {
                        eliminate_leaves(&c.body.statements, accum_ids, next_id)
                    } else {
                        LeaveElimResult {
                            statements: c.body.statements.clone(),
                            accum_ids: accum_ids.to_vec(),
                            done_id: None,
                        }
                    };
                    let case_done = case_result.done_id.unwrap_or(done_false_id);

                    let mut statements = case_result.statements;
                    let mut yields: Vec<Value> = c.body.yields.clone();

                    let top_defs = collect_top_scope_defs(&statements);
                    promote_yields_from_guards(&mut statements, &mut yields, &top_defs, next_id);

                    yields.extend(case_result.accum_ids.iter().map(|&id| Value {
                        id,
                        value_type: Type::Int(BitWidth::I256),
                    }));
                    yields.push(Value {
                        id: case_done,
                        value_type: Type::Int(BitWidth::I256),
                    });
                    SwitchCase {
                        value: c.value.clone(),
                        body: Region { statements, yields },
                    }
                })
                .collect();

            let new_default = default.as_ref().map(|d| {
                let def_result = if statements_have_leave(&d.statements) {
                    eliminate_leaves(&d.statements, accum_ids, next_id)
                } else {
                    LeaveElimResult {
                        statements: d.statements.clone(),
                        accum_ids: accum_ids.to_vec(),
                        done_id: None,
                    }
                };
                let def_done = def_result.done_id.unwrap_or(done_false_id);

                let mut statements = def_result.statements;
                let mut yields: Vec<Value> = d.yields.clone();

                let top_defs = collect_top_scope_defs(&statements);
                promote_yields_from_guards(&mut statements, &mut yields, &top_defs, next_id);

                yields.extend(def_result.accum_ids.iter().map(|&id| Value {
                    id,
                    value_type: Type::Int(BitWidth::I256),
                }));
                yields.push(Value {
                    id: def_done,
                    value_type: Type::Int(BitWidth::I256),
                });
                Region { statements, yields }
            });

            let new_accums: Vec<ValueId> = accum_ids.iter().map(|_| fresh_id(next_id)).collect();
            let new_done = fresh_id(next_id);
            let mut all_outputs: Vec<ValueId> = orig_outputs.clone();
            all_outputs.extend(new_accums.iter());
            all_outputs.push(new_done);

            let mut inputs: Vec<Value> = orig_inputs.clone();
            inputs.extend(accum_ids.iter().map(|&id| Value {
                id,
                value_type: Type::Int(BitWidth::I256),
            }));
            inputs.push(Value {
                id: done_false_id,
                value_type: Type::Int(BitWidth::I256),
            });

            pre_stmts.push(Statement::Switch {
                scrutinee: *scrutinee,
                inputs,
                cases: new_cases,
                default: new_default,
                outputs: all_outputs,
            });

            LeaveElimResult {
                statements: pre_stmts,
                accum_ids: new_accums,
                done_id: Some(new_done),
            }
        }

        Statement::Block(region) => eliminate_leaves(&region.statements, accum_ids, next_id),

        _ => LeaveElimResult {
            statements: vec![statement.clone()],
            accum_ids: accum_ids.to_vec(),
            done_id: None,
        },
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

    let analysis = analyze_call_graph(object);

    for (&func_id, function) in object.functions.iter_mut() {
        function.call_count = analysis.call_counts.get(&func_id).copied().unwrap_or(0);
    }

    let decisions = make_inline_decisions(object, &analysis);
    results.decisions = decisions.clone();

    let mut next_value_id = ValueId(object.find_max_value_id() + 1);

    let functions_snapshot: BTreeMap<FunctionId, Function> = object.functions.clone();

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

    let new_code_stmts = inline_in_statements(
        &object.code.statements,
        &inlineable,
        &functions_snapshot,
        &mut next_value_id,
        &mut results.inlined_call_sites,
    );
    object.code.statements = new_code_stmts;

    let func_ids: Vec<FunctionId> = object.functions.keys().copied().collect();
    for func_id in func_ids {
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
    next_value_id: &mut ValueId,
    inlined_count: &mut usize,
) -> Vec<Statement> {
    let mut result = Vec::new();

    for statement in statements {
        match statement {
            Statement::Let {
                bindings,
                value:
                    Expression::Call {
                        function,
                        arguments,
                    },
            } if inlineable.contains(function) => {
                if let Some(func_def) = functions.get(function) {
                    let inlined =
                        inline_call_with_results(func_def, arguments, bindings, next_value_id);
                    result.extend(inlined);
                    *inlined_count += 1;
                } else {
                    result.push(statement.clone());
                }
            }

            Statement::Expression(Expression::Call {
                function,
                arguments,
            }) if inlineable.contains(function) => {
                if let Some(func_def) = functions.get(function) {
                    let inlined = inline_call_void(func_def, arguments, next_value_id);
                    result.extend(inlined);
                    *inlined_count += 1;
                } else {
                    result.push(statement.clone());
                }
            }

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
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
            } => {
                let new_cond_stmts = inline_in_statements(
                    condition_statements,
                    inlineable,
                    functions,
                    next_value_id,
                    inlined_count,
                );
                result.push(Statement::For {
                    initial_values: initial_values.clone(),
                    loop_variables: loop_variables.clone(),
                    condition_statements: new_cond_stmts,
                    condition: condition.clone(),
                    body: inline_in_region(
                        body,
                        inlineable,
                        functions,
                        next_value_id,
                        inlined_count,
                    ),
                    post_input_variables: post_input_variables.clone(),
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
    next_value_id: &mut ValueId,
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
    arguments: &[Value],
    result_bindings: &[ValueId],
    next_value_id: &mut ValueId,
) -> Vec<Statement> {
    let mut remapper = InlineRemapper::new(*next_value_id);
    let mut statements = Vec::new();

    debug_assert_eq!(
        function.parameters.len(),
        arguments.len(),
        "inliner: argument count mismatch for function (expected {}, got {})",
        function.parameters.len(),
        arguments.len()
    );
    for ((parameter_id, _param_ty), argument) in function.parameters.iter().zip(arguments.iter()) {
        let new_parameter_id = remapper.remap_value_id(*parameter_id);
        statements.push(Statement::Let {
            bindings: vec![new_parameter_id],
            value: Expression::Var(argument.id),
        });
    }

    for &ret_init_id in &function.return_values_initial {
        let new_ret_id = remapper.remap_value_id(ret_init_id);
        statements.push(Statement::Let {
            bindings: vec![new_ret_id],
            value: Expression::Literal {
                value: num::BigUint::from(0u32),
                value_type: Type::Int(BitWidth::I256),
            },
        });
    }

    let remapped_body = remapper.remap_statements(&function.body.statements);
    *next_value_id = remapper.next_value_id;

    let initial_accums: Vec<ValueId> = function
        .return_values_initial
        .iter()
        .filter_map(|id| remapper.value_map.get(id).copied())
        .collect();
    let fallthrough_ids: Vec<ValueId> = function
        .return_values
        .iter()
        .filter_map(|id| remapper.value_map.get(id).copied())
        .collect();

    let mut body_with_leave = remapped_body;
    body_with_leave.push(Statement::Leave {
        return_values: fallthrough_ids
            .iter()
            .map(|&id| Value {
                id,
                value_type: Type::Int(BitWidth::I256),
            })
            .collect(),
    });

    let elim = eliminate_leaves(&body_with_leave, &initial_accums, next_value_id);
    statements.extend(elim.statements);

    for (result_binding, final_ret) in result_bindings.iter().zip(elim.accum_ids.iter()) {
        statements.push(Statement::Let {
            bindings: vec![*result_binding],
            value: Expression::Var(*final_ret),
        });
    }

    statements
}

/// Inlines a function call whose result is discarded.
fn inline_call_void(
    function: &Function,
    arguments: &[Value],
    next_value_id: &mut ValueId,
) -> Vec<Statement> {
    let mut remapper = InlineRemapper::new(*next_value_id);
    let mut statements = Vec::new();

    for ((parameter_id, _), argument) in function.parameters.iter().zip(arguments.iter()) {
        let new_parameter_id = remapper.remap_value_id(*parameter_id);
        statements.push(Statement::Let {
            bindings: vec![new_parameter_id],
            value: Expression::Var(argument.id),
        });
    }

    for &ret_init_id in &function.return_values_initial {
        let new_ret_id = remapper.remap_value_id(ret_init_id);
        statements.push(Statement::Let {
            bindings: vec![new_ret_id],
            value: Expression::Literal {
                value: num::BigUint::from(0u32),
                value_type: Type::Int(BitWidth::I256),
            },
        });
    }

    let remapped_body = remapper.remap_statements(&function.body.statements);
    *next_value_id = remapper.next_value_id;

    if statements_have_leave(&remapped_body) {
        let initial_accums: Vec<ValueId> = function
            .return_values_initial
            .iter()
            .filter_map(|id| remapper.value_map.get(id).copied())
            .collect();

        let elim = eliminate_leaves(&remapped_body, &initial_accums, next_value_id);
        statements.extend(elim.statements);
    } else {
        statements.extend(remapped_body);
    }

    statements
}

/// Estimates the size of a block (same metric used by from_yul.rs).
pub(crate) fn estimate_block_size(block: &Block) -> usize {
    block.statements.iter().map(estimate_statement_size).sum()
}

/// Estimates the size of a statement.
fn estimate_statement_size(statement: &Statement) -> usize {
    match statement {
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
        Statement::Expression(_) => 1,
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

        let mut next_id = id * 1000;
        for _ in 0..num_params {
            function
                .parameters
                .push((ValueId::new(next_id), Type::Int(BitWidth::I256)));
            next_id += 1;
        }

        for _ in 0..num_returns {
            function.returns.push(Type::Int(BitWidth::I256));
            function.return_values_initial.push(ValueId::new(next_id));
            next_id += 1;
        }

        for i in 0..num_stmts {
            function.body.push(Statement::Let {
                bindings: vec![ValueId::new(next_id)],
                value: Expression::Literal {
                    value: BigUint::from(i as u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            });
            next_id += 1;
        }

        if num_returns > 0 {
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
        let mut object = Object::new("test".to_string());

        let g = make_simple_function(1, "g", 3, 0, 1);
        let g_id = g.id;

        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Let {
            bindings: vec![ValueId::new(100)],
            value: Expression::Call {
                function: g_id,
                arguments: vec![],
            },
        });
        f.body.push(Statement::Let {
            bindings: vec![ValueId::new(101)],
            value: Expression::Call {
                function: g_id,
                arguments: vec![],
            },
        });
        f.size_estimate = 2;
        let f_id = f.id;

        object.functions.insert(f_id, f);
        object.functions.insert(g_id, g);

        object.code.push(Statement::Expression(Expression::Call {
            function: f_id,
            arguments: vec![],
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

        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(0),
            arguments: vec![],
        }));
        f.size_estimate = 1;
        object.functions.insert(f.id, f);

        let analysis = analyze_call_graph(&object);
        assert!(analysis.recursive_functions.contains(&FunctionId::new(0)));
    }

    #[test]
    fn test_mutual_recursion_detection() {
        let mut object = Object::new("test".to_string());

        let mut f = Function::new(FunctionId::new(0), "f".to_string());
        f.body.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(1),
            arguments: vec![],
        }));
        f.size_estimate = 1;

        let mut g = Function::new(FunctionId::new(1), "g".to_string());
        g.body.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(0),
            arguments: vec![],
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

        let mut g = Function::new(FunctionId::new(1), "g".to_string());
        let g_ret_init = ValueId::new(1000);
        let g_ret_final = ValueId::new(1001);
        g.returns.push(Type::Int(BitWidth::I256));
        g.return_values_initial.push(g_ret_init);
        g.body.push(Statement::Let {
            bindings: vec![g_ret_final],
            value: Expression::Literal {
                value: BigUint::from(42u32),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        g.return_values.push(g_ret_final);
        g.size_estimate = 1;
        g.call_count = 0;

        object.functions.insert(g.id, g);

        object.code.push(Statement::Let {
            bindings: vec![ValueId::new(99)],
            value: Expression::Call {
                function: FunctionId::new(1),
                arguments: vec![],
            },
        });

        let results = inline_functions(&mut object);

        assert_eq!(results.inlined_call_sites, 1);
        assert!(results.removed_functions.contains(&FunctionId::new(1)));
        assert!(!object.functions.contains_key(&FunctionId::new(1)));

        assert!(object.code.statements.len() > 1);
        let has_call = object.code.statements.iter().any(|s| {
            matches!(
                s,
                Statement::Let {
                    value: Expression::Call { .. },
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
        f.body.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(0),
            arguments: vec![],
        }));
        f.size_estimate = 1;
        object.functions.insert(f.id, f);

        object.code.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(0),
            arguments: vec![],
        }));

        let results = inline_functions(&mut object);

        assert_eq!(results.inlined_call_sites, 0);
        assert!(object.functions.contains_key(&FunctionId::new(0)));
    }

    #[test]
    fn test_inline_decisions() {
        let mut object = Object::new("test".to_string());

        let small = make_simple_function(0, "small", 3, 0, 0);
        object.functions.insert(small.id, small);

        let medium = make_simple_function(1, "medium", 50, 0, 0);
        object.functions.insert(medium.id, medium);

        let large = make_simple_function(2, "large", 150, 0, 0);
        object.functions.insert(large.id, large);

        object.code.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(0),
            arguments: vec![],
        }));
        for _ in 0..15 {
            object.code.push(Statement::Expression(Expression::Call {
                function: FunctionId::new(1),
                arguments: vec![],
            }));
        }
        object.code.push(Statement::Expression(Expression::Call {
            function: FunctionId::new(2),
            arguments: vec![],
        }));

        let analysis = analyze_call_graph(&object);
        let decisions = make_inline_decisions(&object, &analysis);

        assert_eq!(
            decisions.get(&FunctionId::new(0)),
            Some(&InlineDecision::AlwaysInline)
        );
        // medium (size 50, 15 callers): with NEVER_INLINE_CALL_COUNT_THRESHOLD
        // now at 100, this drops to the cost-benefit branch which rejects it
        // (cost 700, benefit 0) so it ends up CostBenefit rather than
        // NeverInline. Both effectively keep the body intact and tell LLVM
        // not to inline; the distinction matters only for trace dumps.
        assert_eq!(
            decisions.get(&FunctionId::new(1)),
            Some(&InlineDecision::CostBenefit)
        );
        assert_eq!(
            decisions.get(&FunctionId::new(2)),
            Some(&InlineDecision::NeverInline)
        );
    }
}
