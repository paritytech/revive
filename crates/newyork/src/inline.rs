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
use crate::simplify::Simplifier;

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

impl std::ops::AddAssign for InlineResults {
    fn add_assign(&mut self, rhs: Self) {
        self.inlined_call_sites += rhs.inlined_call_sites;
        self.removed_functions.extend(rhs.removed_functions);
        self.decisions.extend(rhs.decisions);
    }
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
            Statement::ExternalCall { result, .. } | Statement::Create { result, .. } => {
                definitions.push(*result);
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
            Statement::ExternalCall { result, .. } | Statement::Create { result, .. }
                if *result == target =>
            {
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
    statements: &mut Vec<Statement>,
    yields: &mut [Value],
    top_defs: &[ValueId],
    next_id: &mut ValueId,
) {
    for yield_val in yields.iter_mut() {
        if top_defs.contains(&yield_val.id) {
            continue;
        }
        promote_one_yield(statements, yield_val, next_id);
    }
}

/// Promotes a single value to be a top-scope output of the guard `If` that defines it,
/// rewriting `yield_val.id` to that output. No-op if no guard in `statements` defines it.
///
/// Promotion recurses through *nested* guards: a value defined two or more `leave`-guards deep is
/// promoted out of each level in turn, so the value pushed into a region's `yields` is always
/// defined at that region's top scope (otherwise the yield would reference a value defined inside
/// a deeper region — a use-before-definition the validator rejects).
///
/// Each new output needs a false-edge value (taken when the guard is skipped, i.e. a `leave`
/// fired). That value is dead — every consumer of a promoted value is itself guarded by the same
/// `done` flag, so it never runs on the skipped edge — and only has to *dominate* the `If`. We
/// reuse an existing input when one exists, otherwise insert a zero before the guard, so void
/// functions (no return accumulators, hence no inputs) still get a dominating placeholder rather
/// than referencing the not-yet-defined value itself.
fn promote_one_yield(
    statements: &mut Vec<Statement>,
    yield_val: &mut Value,
    next_id: &mut ValueId,
) {
    let target = yield_val.id;
    let Some(mut if_index) = statements.iter().position(|statement| {
        matches!(statement, Statement::If { then_region, .. }
            if region_defines_value(then_region, target))
    }) else {
        return;
    };

    // Make sure `target` is defined at the guard's top scope before yielding it; if it lives in a
    // deeper nested guard, promote it out of that one first.
    let promoted = {
        let Statement::If { then_region, .. } = &mut statements[if_index] else {
            unreachable!("position matched an If")
        };
        if collect_top_scope_defs(&then_region.statements).contains(&target) {
            *yield_val
        } else {
            let mut inner = *yield_val;
            promote_one_yield(&mut then_region.statements, &mut inner, next_id);
            inner
        }
    };

    // Choose a dominating false-edge placeholder.
    let existing_input = match &statements[if_index] {
        Statement::If { inputs, .. } => inputs.first().copied(),
        _ => unreachable!(),
    };
    let placeholder = existing_input.unwrap_or_else(|| {
        let zero = fresh_id(next_id);
        statements.insert(
            if_index,
            Statement::Let {
                bindings: vec![zero],
                value: Expression::Literal {
                    value: num::BigUint::from(0u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
        );
        if_index += 1;
        Value {
            id: zero,
            value_type: Type::Int(BitWidth::I256),
        }
    });

    let Statement::If {
        then_region,
        inputs,
        outputs,
        ..
    } = &mut statements[if_index]
    else {
        unreachable!()
    };
    then_region.yields.push(promoted);
    let new_out = fresh_id(next_id);
    outputs.push(new_out);
    inputs.push(placeholder);
    yield_val.id = new_out;
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

/// Per-call-site overhead in IR units used by [`inline_by_shrink_prediction`]. Roughly maps to the
/// LLVM cost of a `tail call` + per-arg setup. `make_inline_decisions` treats `1 IR unit ≈ 1 LLVM
/// byte` empirically, so a small constant here is correct.
pub(crate) const SHRINK_PREDICTION_CALL_OVERHEAD: usize = 2;

/// Minimum body size considered by [`inline_by_shrink_prediction`]. Below the always-inline
/// threshold the existing inliner already inlines, so shrink-prediction adds no information.
pub(crate) const SHRINK_PREDICTION_MIN_CANDIDATE_SIZE: usize = 7;

/// Maximum body size considered by [`inline_by_shrink_prediction`]. Beyond this, body-side
/// simplifier folding cannot realistically halve a 70+ statement function via a handful of literal
/// args, so prediction is fragile and disabled.
pub(crate) const SHRINK_PREDICTION_MAX_CANDIDATE_SIZE: usize = 60;

/// Minimum number of call sites a function needs before [`inline_by_shrink_prediction`]
/// considers it. Single-call functions are already handled by the canonical inliner's
/// single-call path, so prediction only adds value for multi-call functions.
pub(crate) const SHRINK_PREDICTION_MIN_CALL_SITES: usize = 2;

/// Benefit threshold for [`inline_by_shrink_prediction`], expressed as a fraction:
/// inline only when the predicted post-substitution size is below
/// `NUMERATOR / DENOMINATOR` (= 3/4 = 75%) of the cost of keeping the function. The
/// `1-IR-unit ≈ 1-LLVM-byte` assumption is tight, so small predicted wins are noise;
/// empirically 60% changes nothing on the OZ corpus and 80% admits +868 bytes of regression.
pub(crate) const SHRINK_PREDICTION_BENEFIT_NUMERATOR: usize = 3;
/// Denominator of [`SHRINK_PREDICTION_BENEFIT_NUMERATOR`].
pub(crate) const SHRINK_PREDICTION_BENEFIT_DENOMINATOR: usize = 4;

/// Size estimate assigned to a function chosen for shrink-prediction inlining, so the
/// following [`inline_functions`] pass sees it below [`ALWAYS_INLINE_SIZE_THRESHOLD`] and
/// inlines it through the canonical path rather than duplicating that logic here.
pub(crate) const SHRINK_PREDICTION_FORCED_INLINE_SIZE: usize = 1;

/// Eliminate parameters whose argument is the same compile-time literal at
/// every call site. The parameter is removed from the function signature, an
/// equivalent `Let` binding is prepended to the body so existing uses of the
/// parameter's `ValueId` keep working, and the matching argument is dropped
/// from every call expression.
///
/// Returns the total number of `(function, parameter)` pairs eliminated
/// (including those inside subobjects).
///
/// This is an IR-level analogue of LLVM's `deadargelim` + `ipsccp` constant
/// propagation through call boundaries — necessary because most newyork
/// helpers carry `noinline` for code-size reasons, which blocks LLVM's IPO
/// from baking constants into the function bodies.
pub(crate) fn eliminate_constant_parameters(object: &mut Object) -> usize {
    use std::collections::BTreeMap;

    let mut total = 0;
    for subobject in &mut object.subobjects {
        total += eliminate_constant_parameters(subobject);
    }

    let mut block_literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    collect_top_level_literals(&object.code.statements, &mut block_literals);

    #[derive(Clone)]
    enum ArgState {
        Initial,
        Constant(num::BigUint, Type),
        Variable,
    }

    let mut arg_states: BTreeMap<(FunctionId, usize), ArgState> = BTreeMap::new();
    let mut call_counts: BTreeMap<FunctionId, usize> = BTreeMap::new();
    for &function_id in object.functions.keys() {
        call_counts.insert(function_id, 0);
    }

    let mut record_call =
        |function_id: FunctionId,
         arguments: &[Value],
         extra_literals: &BTreeMap<u32, (num::BigUint, Type)>| {
            *call_counts.entry(function_id).or_insert(0) += 1;
            for (parameter_index, argument) in arguments.iter().enumerate() {
                let new_state = match extra_literals
                    .get(&argument.id.0)
                    .or_else(|| block_literals.get(&argument.id.0))
                {
                    Some((value, value_type)) => ArgState::Constant(value.clone(), *value_type),
                    None => ArgState::Variable,
                };
                let entry = arg_states
                    .entry((function_id, parameter_index))
                    .or_insert(ArgState::Initial);
                let merged = match (entry.clone(), new_state) {
                    (ArgState::Initial, fresh) => fresh,
                    (ArgState::Variable, _) | (_, ArgState::Variable) => ArgState::Variable,
                    (ArgState::Constant(value_a, type_a), ArgState::Constant(value_b, type_b))
                        if value_a == value_b && type_a == type_b =>
                    {
                        ArgState::Constant(value_a, type_a)
                    }
                    _ => ArgState::Variable,
                };
                *entry = merged;
            }
        };

    visit_calls(&object.code.statements, &mut record_call);
    for function in object.functions.values() {
        visit_calls(&function.body.statements, &mut record_call);
    }

    let mut by_function: BTreeMap<FunctionId, Vec<(usize, num::BigUint, Type)>> = BTreeMap::new();
    for ((function_id, parameter_index), state) in arg_states {
        if call_counts.get(&function_id).copied().unwrap_or(0) < 1 {
            continue;
        }
        if let ArgState::Constant(value, value_type) = state {
            by_function
                .entry(function_id)
                .or_default()
                .push((parameter_index, value, value_type));
        }
    }

    if by_function.is_empty() {
        return total;
    }

    let mut drop_indices: BTreeMap<FunctionId, Vec<usize>> = BTreeMap::new();

    for (function_id, mut entries) in by_function {
        let Some(function) = object.functions.get_mut(&function_id) else {
            continue;
        };

        // Don't eliminate every parameter — leave the function with at least
        // one input slot so the call-site arity matches non-zero. (Yul allows
        // zero-arg functions, so this isn't strictly necessary, but it keeps
        // the change conservative and the diff smaller.)
        if entries.len() >= function.parameters.len() {
            entries.truncate(function.parameters.len().saturating_sub(1));
            if entries.is_empty() {
                continue;
            }
        }

        entries.sort_by_key(|entry| entry.0);
        let mut prologue: Vec<Statement> = Vec::with_capacity(entries.len());
        for (parameter_index, value, value_type) in &entries {
            let (parameter_id, _) = function.parameters[*parameter_index];
            prologue.push(Statement::Let {
                bindings: vec![parameter_id],
                value: Expression::Literal {
                    value: value.clone(),
                    value_type: *value_type,
                },
            });
        }

        let dropped: Vec<usize> = entries.iter().map(|(index, _, _)| *index).collect();
        let mut dropped_descending = dropped.clone();
        dropped_descending.sort_by(|a, b| b.cmp(a));
        for parameter_index in dropped_descending {
            function.parameters.remove(parameter_index);
        }

        let mut new_body = prologue;
        new_body.extend(std::mem::take(&mut function.body.statements));
        function.body.statements = new_body;
        function.size_estimate = estimate_block_size(&function.body);

        total += entries.len();
        drop_indices.insert(function_id, dropped);
    }

    if drop_indices.is_empty() {
        return total;
    }

    trim_call_arguments(&mut object.code.statements, &drop_indices);
    for function in object.functions.values_mut() {
        trim_call_arguments(&mut function.body.statements, &drop_indices);
    }

    total
}

/// Inlines functions whose predicted post-substitution size (with
/// callsite literal arguments propagated through the body and a fresh
/// simplifier pass run on the result) beats the cost of keeping them.
///
/// For each function in the cost-benefit range:
/// 1. Walk every call site, build the literal-arg map for that site.
/// 2. Clone the function body, prepend `Let p_i = Literal(value)` for each
///    literal arg, drop those params from the signature.
/// 3. Run the full simplifier on the cloned function in isolation.
/// 4. Measure the resulting `size_estimate`.
/// 5. Sum the predicted sizes over all call sites.
/// 6. If the sum (no body, no call overhead) beats the keep-cost
///    (body + N × call-overhead), force-inline by lowering
///    `size_estimate` so the next `inline_functions` pass picks the
///    function as `AlwaysInline`.
///
/// Returns the number of functions force-inlined.
pub(crate) fn inline_by_shrink_prediction(object: &mut Object) -> usize {
    use std::collections::BTreeMap;

    let mut total = 0;
    for subobject in &mut object.subobjects {
        total += inline_by_shrink_prediction(subobject);
    }

    let mut block_literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    collect_top_level_literals(&object.code.statements, &mut block_literals);

    // For each (function, callsite) → ordered list of (param_idx → literal)
    // for the args known at the call site.
    type LiteralArgs = BTreeMap<usize, (num::BigUint, Type)>;
    let mut sites_per_callee: BTreeMap<FunctionId, Vec<LiteralArgs>> = BTreeMap::new();

    let mut record_call =
        |function_id: FunctionId,
         arguments: &[Value],
         extra_literals: &BTreeMap<u32, (num::BigUint, Type)>| {
            let mut site_literals: LiteralArgs = BTreeMap::new();
            for (parameter_index, argument) in arguments.iter().enumerate() {
                if let Some((value, value_type)) = extra_literals
                    .get(&argument.id.0)
                    .or_else(|| block_literals.get(&argument.id.0))
                {
                    site_literals.insert(parameter_index, (value.clone(), *value_type));
                }
            }
            sites_per_callee
                .entry(function_id)
                .or_default()
                .push(site_literals);
        };

    visit_calls(&object.code.statements, &mut record_call);
    for function in object.functions.values() {
        visit_calls(&function.body.statements, &mut record_call);
    }

    let mut force_inline: Vec<FunctionId> = Vec::new();
    for (function_id, sites) in &sites_per_callee {
        if sites.len() < SHRINK_PREDICTION_MIN_CALL_SITES {
            continue;
        }
        let Some(function) = object.functions.get(function_id) else {
            continue;
        };
        if function.size_estimate < SHRINK_PREDICTION_MIN_CANDIDATE_SIZE {
            continue;
        }
        if function.size_estimate > SHRINK_PREDICTION_MAX_CANDIDATE_SIZE {
            continue;
        }
        if !sites.iter().any(|literals| !literals.is_empty()) {
            continue;
        }

        let mut total_predicted = 0;
        for site_literals in sites {
            total_predicted += predict_simplified_size(function, site_literals);
        }
        let keep_cost = function.size_estimate + sites.len() * SHRINK_PREDICTION_CALL_OVERHEAD;
        if total_predicted * SHRINK_PREDICTION_BENEFIT_DENOMINATOR
            < keep_cost * SHRINK_PREDICTION_BENEFIT_NUMERATOR
        {
            force_inline.push(*function_id);
        }
    }

    if force_inline.is_empty() {
        return total;
    }

    // Lower size_estimate below ALWAYS_INLINE_SIZE_THRESHOLD so the next
    // `inline_functions` pass picks these as AlwaysInline, reusing the
    // canonical inliner rather than duplicating its logic.
    for function_id in &force_inline {
        if let Some(function) = object.functions.get_mut(function_id) {
            function.size_estimate = SHRINK_PREDICTION_FORCED_INLINE_SIZE;
        }
    }
    let _ = inline_functions(object);
    total += force_inline.len();

    total
}

/// Clones `function`, prepends `Let p_i = Literal(value)` for every
/// `(param_idx, value)` in `literal_args`, drops those parameters from
/// the signature, then runs the full simplifier on the clone in
/// isolation. Returns the post-simplify size estimate.
pub(crate) fn predict_simplified_size(
    function: &Function,
    literal_args: &std::collections::BTreeMap<usize, (num::BigUint, Type)>,
) -> usize {
    let mut temp = function.clone();
    let mut sorted_args: Vec<(usize, num::BigUint, Type)> = literal_args
        .iter()
        .map(|(&idx, (value, value_type))| (idx, value.clone(), *value_type))
        .collect();
    sorted_args.sort_by_key(|entry| entry.0);

    let mut prologue: Vec<Statement> = Vec::with_capacity(sorted_args.len());
    for (idx, value, value_type) in &sorted_args {
        if *idx >= temp.parameters.len() {
            continue;
        }
        let (param_id, _) = temp.parameters[*idx];
        prologue.push(Statement::Let {
            bindings: vec![param_id],
            value: Expression::Literal {
                value: value.clone(),
                value_type: *value_type,
            },
        });
    }
    let mut dropped: Vec<usize> = sorted_args
        .iter()
        .filter(|(idx, _, _)| *idx < temp.parameters.len())
        .map(|(idx, _, _)| *idx)
        .collect();
    dropped.sort_by(|a, b| b.cmp(a));
    for idx in dropped {
        temp.parameters.remove(idx);
    }

    let mut new_body = prologue;
    new_body.extend(std::mem::take(&mut temp.body.statements));
    temp.body.statements = new_body;

    let original_id = temp.id;
    let mut tmp_object = Object::new("tmp_shrink_prediction".to_string());
    tmp_object.functions.insert(original_id, temp);
    let mut simplifier = Simplifier::new();
    simplifier.simplify_object(&mut tmp_object);

    let simplified_function = match tmp_object.functions.get(&original_id) {
        Some(function) => function,
        None => return function.size_estimate,
    };
    estimate_block_size(&simplified_function.body)
}

/// Records `Let id = Literal v` bindings that occur at the top level of a
/// statement list (not inside any nested branch).
pub(crate) fn collect_top_level_literals(
    statements: &[Statement],
    literals: &mut std::collections::BTreeMap<u32, (num::BigUint, Type)>,
) {
    for statement in statements {
        if let Statement::Let {
            bindings,
            value: Expression::Literal { value, value_type },
        } = statement
        {
            if bindings.len() == 1 {
                literals.insert(bindings[0].0, (value.clone(), *value_type));
            }
        }
    }
}

/// Visits every Call expression in a statement list (recursing into nested
/// regions) and invokes `record(function_id, arguments, branch_literals)`.
/// `branch_literals` covers `Let id = Literal v` bindings encountered earlier
/// in the same statement list — branches inherit the literals from the
/// containing block but their own literals stay local to that branch.
pub(crate) fn visit_calls<F>(statements: &[Statement], record: &mut F)
where
    F: FnMut(FunctionId, &[Value], &std::collections::BTreeMap<u32, (num::BigUint, Type)>),
{
    use std::collections::BTreeMap;
    let mut literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    visit_calls_inner(statements, &mut literals, record);
}

/// Recursive worker for [`visit_calls`]. Walks `statements`, recording every
/// top-level call (`Let id = call(..)` and bare `call(..)` statements) and tracking
/// `Let id = Literal v` bindings in `literals`. Control-flow children are visited
/// with a cloned `literals` map so each branch's own bindings stay local while still
/// inheriting the enclosing block's literals.
pub(crate) fn visit_calls_inner<F>(
    statements: &[Statement],
    literals: &mut std::collections::BTreeMap<u32, (num::BigUint, Type)>,
    record: &mut F,
) where
    F: FnMut(FunctionId, &[Value], &std::collections::BTreeMap<u32, (num::BigUint, Type)>),
{
    for statement in statements {
        match statement {
            Statement::Let { bindings, value } => {
                if let Expression::Literal { value, value_type } = value {
                    if bindings.len() == 1 {
                        literals.insert(bindings[0].0, (value.clone(), *value_type));
                    }
                }
                if let Expression::Call {
                    function,
                    arguments,
                } = value
                {
                    record(*function, arguments, literals);
                }
            }
            Statement::Expression(Expression::Call {
                function,
                arguments,
            }) => {
                record(*function, arguments, literals);
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                let saved = literals.clone();
                visit_calls_inner(&then_region.statements, literals, record);
                if let Some(region) = else_region {
                    *literals = saved.clone();
                    visit_calls_inner(&region.statements, literals, record);
                }
                *literals = saved;
            }
            Statement::Switch { cases, default, .. } => {
                let saved = literals.clone();
                for case in cases {
                    visit_calls_inner(&case.body.statements, literals, record);
                    *literals = saved.clone();
                }
                if let Some(region) = default {
                    visit_calls_inner(&region.statements, literals, record);
                }
                *literals = saved;
            }
            Statement::For {
                condition_statements,
                condition,
                body,
                post,
                ..
            } => {
                let saved = literals.clone();
                visit_calls_inner(condition_statements, literals, record);
                // Unlike `If`'s condition (a `Value`), a `For` condition is an
                // `Expression` and may itself be a top-level call, evaluated after
                // `condition_statements`. Record it so its arguments are analysed.
                if let Expression::Call {
                    function,
                    arguments,
                } = condition
                {
                    record(*function, arguments, literals);
                }
                visit_calls_inner(&body.statements, literals, record);
                visit_calls_inner(&post.statements, literals, record);
                *literals = saved;
            }
            Statement::Block(region) => {
                let saved = literals.clone();
                visit_calls_inner(&region.statements, literals, record);
                *literals = saved;
            }
            _ => {}
        }
    }
}

/// Trim arguments from every Call expression in the statement list whose
/// callee appears in `drops`. `drops[fid]` is the ascending list of argument
/// indices to remove.
pub(crate) fn trim_call_arguments(
    statements: &mut [Statement],
    drops: &std::collections::BTreeMap<FunctionId, Vec<usize>>,
) {
    for statement in statements.iter_mut() {
        match statement {
            Statement::Let {
                value:
                    Expression::Call {
                        function,
                        arguments,
                    },
                ..
            } => {
                if let Some(indices) = drops.get(function) {
                    trim_indices(arguments, indices);
                }
            }
            Statement::Expression(Expression::Call {
                function,
                arguments,
            }) => {
                if let Some(indices) = drops.get(function) {
                    trim_indices(arguments, indices);
                }
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                trim_call_arguments(&mut then_region.statements, drops);
                if let Some(region) = else_region {
                    trim_call_arguments(&mut region.statements, drops);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    trim_call_arguments(&mut case.body.statements, drops);
                }
                if let Some(region) = default {
                    trim_call_arguments(&mut region.statements, drops);
                }
            }
            Statement::For {
                condition_statements,
                condition,
                body,
                post,
                ..
            } => {
                trim_call_arguments(condition_statements, drops);
                // A `For` condition is an `Expression` and may be a top-level call;
                // it must be trimmed too, else dropping the callee's parameter leaves
                // this site passing too many arguments.
                if let Expression::Call {
                    function,
                    arguments,
                } = condition
                {
                    if let Some(indices) = drops.get(function) {
                        trim_indices(arguments, indices);
                    }
                }
                trim_call_arguments(&mut body.statements, drops);
                trim_call_arguments(&mut post.statements, drops);
            }
            Statement::Block(region) => {
                trim_call_arguments(&mut region.statements, drops);
            }
            _ => {}
        }
    }
}

pub(crate) fn trim_indices<T: Clone>(values: &mut Vec<T>, indices_ascending: &[usize]) {
    let mut keep: Vec<T> =
        Vec::with_capacity(values.len() - indices_ascending.len().min(values.len()));
    let drop_set: std::collections::BTreeSet<usize> = indices_ascending.iter().copied().collect();
    for (i, v) in values.iter().enumerate() {
        if !drop_set.contains(&i) {
            keep.push(v.clone());
        }
    }
    *values = keep;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CallKind, CreateKind};
    use num::BigUint;

    /// `collect_top_scope_defs` and `region_defines_value` must recognize the result values of
    /// `ExternalCall`/`Create` statements as definitions. Omitting them made leave-elimination
    /// yield promotion fail to locate a yield defined by a call/create result, producing a
    /// use-before-definition ICE (via the late inliner). Mirrors `Statement::for_each_value_id_def`.
    #[test]
    fn def_scans_include_call_and_create_results() {
        fn val(id: u32) -> Value {
            Value::new(ValueId::new(id), Type::Int(BitWidth::I256))
        }
        let call_result = ValueId::new(100);
        let create_result = ValueId::new(101);

        let statements = vec![
            Statement::ExternalCall {
                kind: CallKind::Call,
                gas: val(1),
                address: val(2),
                value: Some(val(3)),
                args_offset: val(4),
                args_length: val(5),
                ret_offset: val(6),
                ret_length: val(7),
                result: call_result,
            },
            Statement::Create {
                kind: CreateKind::Create,
                value: val(8),
                offset: val(9),
                length: val(10),
                salt: None,
                result: create_result,
            },
        ];

        let defs = collect_top_scope_defs(&statements);
        assert!(
            defs.contains(&call_result),
            "ExternalCall result must be a collected top-scope def"
        );
        assert!(
            defs.contains(&create_result),
            "Create result must be a collected top-scope def"
        );

        let region = Region {
            statements,
            yields: Vec::new(),
        };
        assert!(region_defines_value(&region, call_result));
        assert!(region_defines_value(&region, create_result));
        assert!(!region_defines_value(&region, ValueId::new(999)));
    }

    /// Leave-elimination yield promotion must not produce a use-before-definition (a validator
    /// ICE) for two shapes: a value defined inside *nested* guards (sequential `leave`s), and a
    /// *void* function whose guard has no inputs. Both are single-call so the inliner runs leave
    /// elimination on them; `translate_yul_object` runs the validator and panics on broken IR.
    #[test]
    fn leave_elimination_nested_and_void_no_ice() {
        use revive_yul::lexer::Lexer;
        use revive_yul::parser::statement::object::Object as YulObject;

        // `x` is assigned two guard levels deep, after two sequential `leave`s.
        let nested = r#"
object "T" {
    code {
        function f(cond, a, b) -> out {
            let x := 1
            if cond { if a { leave } if b { leave } x := 7 }
            out := x
        }
        sstore(0, f(calldataload(0), calldataload(32), calldataload(64)))
    }
    object "T_deployed" { code { stop() } }
}
"#;
        // Void function (no returns, hence the guard has no inputs) with a `leave`.
        let void = r#"
object "T" {
    code {
        function g(a, b) {
            let x := 1
            if a { if b { leave } x := 2 }
            sstore(0, x)
        }
        g(calldataload(0), calldataload(32))
    }
    object "T_deployed" { code { stop() } }
}
"#;

        for source in [nested, void] {
            let mut lexer = Lexer::new(source.to_owned());
            let yul_object =
                YulObject::parse(&mut lexer, None).expect("the Yul object should parse");
            crate::translate_yul_object(&yul_object, None)
                .expect("translation should succeed without a validator ICE");
        }
    }

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
