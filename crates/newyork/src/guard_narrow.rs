//! Guard-based value narrowing pass.
//!
//! Detects patterns like `if gt(value, MASK) { <terminates> }` and inserts
//! `let val_narrow = and(value, MASK)` after the guard, replacing all subsequent
//! uses of `value` with `val_narrow`. This gives type inference proof that the
//! value fits in fewer bits, enabling downstream narrowing of comparisons,
//! arithmetic, and memory operations.
//!
//! Common pattern in Solidity ABI decoding and checked arithmetic:
//! ```text
//! let check = gt(calldataload_value, 0xFFFFFFFFFFFFFFFF)
//! if check { revert(0, 0) }
//! // After here, calldataload_value <= UINT64_MAX
//! let offset = add(calldataload_value, 4)  // can be i64 arithmetic
//! ```

use crate::ir::*;
use num::{BigUint, Zero};
use std::collections::BTreeMap;

/// Statistics from guard narrowing.
#[derive(Default, Debug)]
pub struct GuardNarrowStats {
    /// Number of guard patterns detected and narrowed.
    pub guards_narrowed: usize,
    /// Number of value uses replaced with narrowed versions.
    pub uses_replaced: usize,
}

/// Runs guard narrowing on an entire object tree (including subobjects).
pub fn narrow_guards_in_object(object: &mut Object) -> GuardNarrowStats {
    let mut next_id = object.find_max_value_id() + 1;
    let mut stats = GuardNarrowStats::default();

    // Pre-scan: detect functions that always terminate (panic helpers, etc.).
    // The narrower needs to recognize `if guard { panic_call() }` as a guard
    // pattern just like `if guard { revert(0,0) }`. Without this, the
    // canonical Solidity finalize_allocation overflow check —
    // `if or(gt(sum, UINT64_MAX), lt(sum, addend)) { panic_error_0x41() }` —
    // never narrows the addition result.
    let noreturn = detect_noreturn_functions(object);

    // Pre-scan: detect validator functions (void, single param, contains
    // eq-based guard pattern like `if iszero(eq(param, and(param, MASK))) { revert }`).
    // These prove their parameter fits in MASK bits, but the proof doesn't
    // propagate to callers. We record (function_id → mask) so that call sites
    // can insert narrowing ANDs.
    let validators = detect_validator_functions(object, &noreturn);

    let _ = narrow_block(
        &mut object.code,
        &mut next_id,
        &mut stats,
        &validators,
        &noreturn,
    );
    for function in object.functions.values_mut() {
        let replacements = narrow_block(
            &mut function.body,
            &mut next_id,
            &mut stats,
            &validators,
            &noreturn,
        );
        // Apply replacements to function return values. Without this, functions
        // like abi_decode_address return the unmasked value (i256) instead of
        // the AND-masked value (i160), blocking return type narrowing.
        if !replacements.is_empty() {
            for return_value in &mut function.return_values {
                if let Some(&new_id) = replacements.get(&return_value.0) {
                    *return_value = new_id;
                }
            }
        }
    }

    // Body-level rewrite for the canonical Solidity allocator: insert an
    // explicit `if gt(p1, UINT64_MAX) { panic_0x41 }` at the top of every
    // `finalize_allocation`-shaped function and replace subsequent uses of
    // `p1` with `and(p1, UINT64_MAX)`. The function signature stays at
    // `(i64, i256)` so caller IR is unchanged, but the body now exposes
    // an explicit i64 bound on `p1` to the rest of the pipeline. The
    // existing `or(gt(sum, UINT64_MAX), lt(sum, p0))` overflow check
    // becomes structurally redundant in the i64 regime — LLVM/IPSCCP can
    // recognise that and shrink the body without changing semantics.
    rewrite_allocator_p1_with_early_check(object, &mut next_id, &noreturn);

    // Pattern-based parameter narrowing for the canonical Solidity
    // allocator. After guard_narrow has applied addend narrowing inside
    // `finalize_allocation`-shaped functions (`add(p0, p1)` followed by an
    // `if or(gt(sum, UINT64_MAX), lt(sum, p0)) { panic }` overflow check),
    // both parameters provably fit in i64 — yet `narrow_function_params` can't
    // see this because the lt/gt comparisons demand I256. Detect the
    // structural shape and narrow the parameters directly. Plain truncation
    // at call sites is sound because the function's own overflow check
    // would have trapped on any caller that passed a wide value, and the
    // truncation happens in the caller's frame.
    narrow_allocator_param_types(object, &noreturn);

    for sub in &mut object.subobjects {
        let sub_stats = narrow_guards_in_object(sub);
        stats.guards_narrowed += sub_stats.guards_narrowed;
        stats.uses_replaced += sub_stats.uses_replaced;
    }

    stats
}

/// Narrows function parameters that are provably bounded by UINT64_MAX
/// at every reachable path in the function body. Two patterns matter
/// for OZ contracts:
///
/// 1. `(i256, i256) -> ()` with the `finalize_allocation` shape — an
///    add overflow check followed by `mstore(0x40, sum)`.
/// 2. Any param whose first reachable use path is `if gt(p, UINT64_MAX)
///    { <terminates> }` (Solidity's `array_allocation_size_bytes`-shape
///    validator at function entry).
///
/// In both cases, plain truncation at the call site is sound because the
/// function's own guard would have trapped a wide value before the
/// param's first non-validation use.
/// Inserts an explicit `if gt(p1, UINT64_MAX) { panic_0x41 }` early
/// guard plus a downstream `and(p1, UINT64_MAX)` rewrite at the top of
/// each `finalize_allocation`-shaped function. Subsequent uses of `p1`
/// in the body are redirected to the AND-masked value.
///
/// Soundness: the inserted gt/panic pair is *redundant* with the body's
/// existing `or(gt(sum, UINT64_MAX), lt(sum, p0)) { panic_0x41 }` check
/// — any caller that would have failed the early check would also have
/// failed the existing late check, so semantics are preserved. The
/// purpose is to expose the `p1 <= UINT64_MAX` invariant at the *top*
/// of the function so type-inference / LLVM IPSCCP can fold the rest of
/// the body's i256 arithmetic to i64.
///
/// Why this avoids the iter34 regression: the function signature is
/// untouched, so caller-side IR sees the exact same `(i64, i256)`
/// declaration as iter33. Only the *body* changes, and only by adding
/// an early check that any compliant caller already satisfies.
fn rewrite_allocator_p1_with_early_check(
    object: &mut Object,
    next_id: &mut u32,
    _noreturn: &std::collections::BTreeSet<u32>,
) {
    let mut to_rewrite: Vec<u32> = Vec::new();
    for (func_id, function) in &object.functions {
        if function.parameters.len() != 2 || !function.returns.is_empty() {
            continue;
        }
        if !function
            .parameters
            .iter()
            .all(|(_, value_type)| matches!(value_type, Type::Int(BitWidth::I256)))
        {
            continue;
        }
        let p0 = function.parameters[0].0;
        let p1 = function.parameters[1].0;
        if has_allocator_shape(&function.body, p0, p1) {
            to_rewrite.push(func_id.0);
        }
    }

    let uint64_max: BigUint = (BigUint::from(1u32) << 64) - BigUint::from(1u32);
    for func_id in to_rewrite {
        let Some(function) = object.functions.get_mut(&FunctionId(func_id)) else {
            continue;
        };
        let p1 = function.parameters[1].0;
        let max_const = ValueId(*next_id);
        *next_id += 1;
        let gt_check = ValueId(*next_id);
        *next_id += 1;
        let p1_narrow = ValueId(*next_id);
        *next_id += 1;

        let original_stmts = std::mem::take(&mut function.body.statements);

        let mut new_stmts: Vec<Statement> = Vec::with_capacity(original_stmts.len() + 4);
        new_stmts.push(Statement::Let {
            bindings: vec![max_const],
            value: Expression::Literal {
                value: uint64_max.clone(),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        new_stmts.push(Statement::Let {
            bindings: vec![gt_check],
            value: Expression::Binary {
                operation: BinaryOperation::Gt,
                lhs: Value::int(p1),
                rhs: Value::int(max_const),
            },
        });
        new_stmts.push(Statement::If {
            condition: Value::new(gt_check, Type::Int(BitWidth::I1)),
            inputs: Vec::new(),
            then_region: Region {
                statements: vec![Statement::PanicRevert { code: 0x41 }],
                yields: Vec::new(),
            },
            else_region: None,
            outputs: Vec::new(),
        });
        new_stmts.push(Statement::Let {
            bindings: vec![p1_narrow],
            value: Expression::Binary {
                operation: BinaryOperation::And,
                lhs: Value::int(p1),
                rhs: Value::int(max_const),
            },
        });

        let mut replacements: BTreeMap<u32, ValueId> = BTreeMap::new();
        replacements.insert(p1.0, p1_narrow);
        for statement in original_stmts {
            new_stmts.push(replace_value_ids_in_stmt(statement, &replacements));
        }
        function.body.statements = new_stmts;
    }
}

fn narrow_allocator_param_types(object: &mut Object, _noreturn: &std::collections::BTreeSet<u32>) {
    let mut narrow_params: BTreeMap<u32, Vec<usize>> = BTreeMap::new();

    for (func_id, function) in &object.functions {
        if function.parameters.is_empty() {
            continue;
        }

        // Pattern 1: 2-param finalize_allocation shape. Narrow ONLY p0
        // (the FMP-typed addend that's also the lt rhs in the overflow
        // check). The other param can be a user-controlled allocation
        // size — `new uint[](2 << 100)` is a valid Solidity expression
        // that *intentionally* triggers the overflow check; truncating
        // it at the call site would silently make the panic disappear.
        // p0 is always sourced from `mload(0x40)` in OZ contracts and
        // bounded by heap_size, so plain truncation is sound.
        if function.parameters.len() == 2
            && function.returns.is_empty()
            && function
                .parameters
                .iter()
                .all(|(_, value_type)| matches!(value_type, Type::Int(BitWidth::I256)))
        {
            let p0 = function.parameters[0].0;
            let p1 = function.parameters[1].0;
            if has_allocator_shape(&function.body, p0, p1) {
                narrow_params.insert(func_id.0, vec![0]);
                continue;
            }
        }

        // Pattern 2 (`if gt(p, UINT64_MAX) { panic }` entry guard) is
        // intentionally NOT applied here: callers pass user-controlled
        // sizes such as `new uint[](2 << 100)`, and the panic the guard
        // is *supposed* to trigger would be silently lost if we plain-
        // truncated the i256 to i64 at the call site. The
        // try_catch/call/panic and try_catch/create/panic differential
        // tests (cases 7 / 16, panic code 0x41) exercise exactly this
        // path. The body-internal guard_narrow AND-mask (iter25/26) is
        // still inserted, so the function body itself still benefits
        // from narrowed downstream arithmetic — just not via a narrowed
        // signature.
    }

    for (func_id, indices) in narrow_params {
        if let Some(function) = object.functions.get_mut(&FunctionId(func_id)) {
            for i in indices {
                if let Some((_, value_type)) = function.parameters.get_mut(i) {
                    *value_type = Type::Int(BitWidth::I64);
                }
            }
        }
    }
}

/// Detects the canonical allocator pattern in a block:
/// * `mstore(0x40, sum)` (FMP store)
/// * `sum = add(p0, x)` somewhere upstream
/// * `if or(gt(sum, UINT64_MAX), lt(sum, p0)) { <terminates> }` overflow check
/// * `x` is derived from `p1` (we don't constrain the alignment computation
///   precisely; the FMP store + add(p0, _) + overflow check is enough).
fn has_allocator_shape(block: &Block, p0: ValueId, _p1: ValueId) -> bool {
    use num::Zero;

    // Pre-pass: build maps for binary expressions in this block.
    let mut add_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut and_results: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new();
    let mut gt_const_checks: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new();
    let mut lt_against_addend: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut or_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut fmp_store_value: Option<u32> = None;

    for statement in &block.statements {
        if let Statement::Let { bindings, value } = statement {
            if bindings.len() == 1 {
                let bid = bindings[0].0;
                if let Expression::Literal { value: lit_val, .. } = value {
                    constants.insert(bid, lit_val.clone());
                }
                if let Expression::Binary {
                    operation,
                    lhs,
                    rhs,
                } = value
                {
                    match operation {
                        BinaryOperation::Add => {
                            add_results.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        BinaryOperation::And => {
                            // Track `and(x, MASK)` where MASK is a constant.
                            if let Some(mask) = constants.get(&rhs.id.0) {
                                and_results.insert(bid, (lhs.id.0, mask.clone()));
                            } else if let Some(mask) = constants.get(&lhs.id.0) {
                                and_results.insert(bid, (rhs.id.0, mask.clone()));
                            }
                        }
                        BinaryOperation::Gt => {
                            if let Some(c) = constants.get(&rhs.id.0) {
                                gt_const_checks.insert(bid, (lhs.id.0, c.clone()));
                            }
                        }
                        BinaryOperation::Lt => {
                            // `lt(sum, addend)` — record both operands so we
                            // can match `lt(sum, p0)`.
                            lt_against_addend.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        BinaryOperation::Or => {
                            or_results.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Statement::MStore { offset, value, .. } = statement {
            if let Some(off) = constants.get(&offset.id.0) {
                if !off.is_zero() && *off == BigUint::from(0x40u32) {
                    fmp_store_value = Some(value.id.0);
                }
            }
        }
    }

    let Some(fmp_val) = fmp_store_value else {
        return false;
    };

    // Walk back through replacements applied during this guard pass: the
    // mstore value is typically a fresh `and(sum, MASK)` we inserted. Step
    // through the and to recover the original sum.
    let sum_id = match and_results.get(&fmp_val) {
        Some((orig, _mask)) => *orig,
        None => fmp_val,
    };

    // The sum must be `add(p0, x)` (or `add(x, p0)`).
    let Some(&(add_lhs, add_rhs)) = add_results.get(&sum_id) else {
        return false;
    };
    let p0_used = add_lhs == p0.0 || add_rhs == p0.0;
    if !p0_used {
        return false;
    }

    // Check for an overflow guard in the OR chain that contains both
    // `gt(sum, UINT64_MAX)` and `lt(sum, p0)`.
    let uint64_max: BigUint = (BigUint::from(1u32) << 64) - 1u32;
    let mut visit: Vec<u32> = Vec::new();
    let mut have_gt_sum_max = false;
    let mut have_lt_sum_p0 = false;

    for (or_id, (l, r)) in &or_results {
        // Walk: did this or-result get used as an `if` condition somewhere
        // in the block, and is the if a terminator? We don't enforce the
        // last detail here (a noreturn-aware check would, but caller-side
        // guard_narrow has already applied if it's a guard); just verify
        // the OR's operands.
        let _ = or_id;
        visit.push(*l);
        visit.push(*r);
    }
    // Also accept un-or'd direct gt/lt pairs.
    for id in &visit {
        if let Some((value, c)) = gt_const_checks.get(id) {
            if *value == sum_id && *c == uint64_max {
                have_gt_sum_max = true;
            }
        }
        if let Some((sum, addend)) = lt_against_addend.get(id) {
            if *sum == sum_id && *addend == p0.0 {
                have_lt_sum_p0 = true;
            }
        }
    }
    // Also check direct (no-OR) presence in the block.
    for (id, (value, c)) in &gt_const_checks {
        let _ = id;
        if *value == sum_id && *c == uint64_max {
            have_gt_sum_max = true;
        }
    }
    for (id, (sum, addend)) in &lt_against_addend {
        let _ = id;
        if *sum == sum_id && *addend == p0.0 {
            have_lt_sum_p0 = true;
        }
    }

    // Allow the loose form (only one of the two): solc emits both, so
    // require both to be safe.
    have_gt_sum_max && have_lt_sum_p0
}

/// Detects functions that always terminate (no fall-through). A function
/// is noreturn if its body has a single statement that is a known terminator
/// (Revert, PanicRevert, etc.) — the typical compiler-generated panic helper
/// like `function panic_error_0x41() { panic_revert(0x41) }`.
///
/// Calls to noreturn functions are equivalent to inlining the terminator at
/// the call site; `region_terminates` accepts them so guard narrowing can
/// recognize patterns like `if check { panic_error_0x41() }`.
///
/// The set is conservative: only single-statement direct terminators count,
/// not nested or conditional terminators. This rules out functions whose
/// termination depends on dynamic checks the analysis can't prove always
/// fire.
///
/// The body iterates to a fixed point so transitive helpers like
/// `function trampoline() { panic_error_0x41() }` are recognised once the
/// leaf `panic_error_0x41` is itself in the set.
fn detect_noreturn_functions(object: &Object) -> std::collections::BTreeSet<u32> {
    let mut noreturn = std::collections::BTreeSet::new();
    loop {
        let before = noreturn.len();
        for (func_id, function) in &object.functions {
            if noreturn.contains(&func_id.0) || function.body.statements.len() != 1 {
                continue;
            }
            let terminates = match &function.body.statements[0] {
                Statement::Revert { .. }
                | Statement::Return { .. }
                | Statement::Invalid
                | Statement::Stop
                | Statement::SelfDestruct { .. }
                | Statement::PanicRevert { .. }
                | Statement::ErrorStringRevert { .. }
                | Statement::CustomErrorRevert { .. } => true,
                Statement::Expression(Expression::Call {
                    function: callee, ..
                }) => noreturn.contains(&callee.0),
                _ => false,
            };
            if terminates {
                noreturn.insert(func_id.0);
            }
        }
        if noreturn.len() == before {
            break;
        }
    }
    noreturn
}

/// Detects "validator" functions that prove their parameter fits in a mask.
///
/// Pattern: void function with one parameter whose body contains:
/// ```text
/// let masked = and(param, MASK)
/// let eq_check = eq(param, masked)
/// let not_check = iszero(eq_check)
/// if not_check { <terminates> }
/// ```
///
/// Returns a map from FunctionId to the boundary mask (BigUint).
fn detect_validator_functions(
    object: &Object,
    noreturn: &std::collections::BTreeSet<u32>,
) -> BTreeMap<u32, BigUint> {
    let mut validators = BTreeMap::new();

    for (func_id, function) in &object.functions {
        // Must be void (no return values) with exactly one parameter
        if !function.returns.is_empty() || function.parameters.len() != 1 {
            continue;
        }

        let param_id = function.parameters[0].0;
        if let Some(mask) = detect_validator_mask(&function.body, param_id, noreturn) {
            validators.insert(func_id.0, mask);
        }
    }

    validators
}

/// Checks if a block contains the eq-based validator pattern for the given param.
/// Returns the boundary mask if found.
fn detect_validator_mask(
    block: &Block,
    param_id: ValueId,
    noreturn: &std::collections::BTreeSet<u32>,
) -> Option<BigUint> {
    let statements = &block.statements;

    // Track definitions to resolve the pattern
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut and_defs: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new(); // and_id -> (orig_id, mask)
    let mut eq_mask_defs: BTreeMap<u32, BigUint> = BTreeMap::new(); // eq_id -> mask
    let mut iszero_eq_defs: BTreeMap<u32, BigUint> = BTreeMap::new(); // iszero_id -> mask

    for statement in statements {
        if let Statement::Let {
            ref bindings,
            ref value,
        } = statement
        {
            if bindings.len() != 1 {
                continue;
            }
            let bid = bindings[0].0;

            if let Expression::Literal {
                value: ref lit_val, ..
            } = value
            {
                constants.insert(bid, lit_val.clone());
            }

            // Track and(param, MASK)
            if let Expression::Binary {
                operation: BinaryOperation::And,
                ref lhs,
                ref rhs,
            } = value
            {
                let (val_id, mask_id) = (lhs.id.0, rhs.id.0);
                if val_id == param_id.0 {
                    if let Some(mask) = constants.get(&mask_id) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (val_id, mask.clone()));
                        }
                    }
                } else if mask_id == param_id.0 {
                    if let Some(mask) = constants.get(&val_id) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (mask_id, mask.clone()));
                        }
                    }
                }
            }

            // Track eq(param, and_result) or eq(and_result, param)
            if let Expression::Binary {
                operation: BinaryOperation::Eq,
                ref lhs,
                ref rhs,
            } = value
            {
                if let Some((orig_id, ref mask)) = and_defs.get(&rhs.id.0) {
                    if *orig_id == lhs.id.0 {
                        eq_mask_defs.insert(bid, mask.clone());
                    }
                } else if let Some((orig_id, ref mask)) = and_defs.get(&lhs.id.0) {
                    if *orig_id == rhs.id.0 {
                        eq_mask_defs.insert(bid, mask.clone());
                    }
                }
            }

            // Track iszero(eq_check)
            if let Expression::Unary {
                operation: UnaryOperation::IsZero,
                ref operand,
            } = value
            {
                if let Some(mask) = eq_mask_defs.get(&operand.id.0) {
                    iszero_eq_defs.insert(bid, mask.clone());
                }
            }
        }

        // Check for if iszero_check { <terminates> }
        if let Statement::If {
            ref condition,
            ref then_region,
            ref else_region,
            ref outputs,
            ..
        } = statement
        {
            if else_region.is_none()
                && outputs.is_empty()
                && region_terminates(then_region, noreturn)
            {
                if let Some(mask) = iszero_eq_defs.get(&condition.id.0) {
                    return Some(mask.clone());
                }
            }
        }
    }

    None
}

/// Process a block: find guard patterns and insert AND masks.
/// Returns the accumulated replacements map so callers can apply it to
/// function return values and other metadata outside the block.
fn narrow_block(
    block: &mut Block,
    next_id: &mut u32,
    stats: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) -> BTreeMap<u32, ValueId> {
    // First, recurse into nested regions within each statement.
    for statement in &mut block.statements {
        narrow_stmt_regions(statement, next_id, stats, validators, noreturn);
    }

    // Now process this block's top-level statements for guard patterns.
    let statements = std::mem::take(&mut block.statements);
    let mut new_stmts = Vec::with_capacity(statements.len() + 16);

    // Track known constants and gt-comparison definitions.
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    // gt_check_id -> (guarded_value, boundary_mask)
    let mut gt_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    // Track and(value, MASK) definitions for eq-based patterns:
    // and_result_id -> (original_value_id, and_result_id_as_ValueId)
    let mut and_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Track eq(value, and(value, MASK)) definitions:
    // eq_result_id -> original_value_id (to be replaced with and_result after guard)
    let mut eq_mask_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Track iszero(eq_check) definitions:
    // iszero_result_id -> (original_value_id, and_result_id)
    let mut iszero_eq_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Track or(a, b) definitions for combined-check guards like
    // `if or(gt(value, MAX), lt(sum, addend)) { panic }` in finalize_allocation.
    // or_result_id -> (lhs_id, rhs_id)
    let mut or_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    // Track add(a, b) definitions so we can recognise the canonical
    // overflow check pattern `sum = add(a, b); if lt(sum, a) { panic }`
    // and narrow both addends when the sum is bounded.
    // add_result_id -> (lhs_id, rhs_id)
    let mut add_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    // Track lt(sum, addend) definitions for overflow checks.
    // lt_result_id -> (sum_id, addend_id)
    let mut lt_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    // Track lt(value, K) where K is a power-of-two constant. After
    // `if iszero(lt(value, K)) { panic }` falls through, value < K so value
    // fits in `log2(K)` bits and can be narrowed by `and(value, K-1)`.
    // Solidity emits this for enum range checks (`if value >= 8 panic`)
    // and bounded-integer guards.
    // lt_result_id -> (val_value, mask = K-1)
    let mut lt_const_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    // Track iszero(lt_const) so the if-guard can find the original value.
    // iszero_result_id -> (val_value, mask)
    let mut iszero_lt_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    // Accumulated replacements: old_value_id -> new_value_id
    let mut replacements: BTreeMap<u32, ValueId> = BTreeMap::new();

    for statement in statements {
        // Apply accumulated replacements to this statement.
        let statement = if replacements.is_empty() {
            statement
        } else {
            replace_value_ids_in_stmt(statement, &replacements)
        };

        // Track constant definitions (let vN = 0xFFFF...).
        if let Statement::Let {
            ref bindings,
            ref value,
        } = statement
        {
            if bindings.len() == 1 {
                let bid = bindings[0].0;

                if let Expression::Literal {
                    value: ref lit_val, ..
                } = value
                {
                    constants.insert(bid, lit_val.clone());
                }

                // Track gt(value, const_boundary) definitions.
                if let Expression::Binary {
                    operation: BinaryOperation::Gt,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    // Resolve rhs through replacements and check if it's a boundary constant.
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    if let Some(mask) = constants.get(&rhs_id.0) {
                        if is_boundary_mask(mask) {
                            // gt(value, MASK): if true, value > MASK (revert); if false, value <= MASK
                            let guarded = Value {
                                id: resolve_id(lhs.id, &replacements),
                                value_type: lhs.value_type,
                            };
                            gt_defs.insert(bid, (guarded, mask.clone()));
                        }
                    }
                }

                // Track and(value, MASK) where MASK is a boundary mask.
                // Used for eq-based address validation: iszero(eq(value, and(value, MASK)))
                if let Expression::Binary {
                    operation: BinaryOperation::And,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    // Check if either operand is a boundary mask constant.
                    if let Some(mask) = constants.get(&rhs_id.0) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (lhs_id.0, ValueId(bid)));
                        }
                    } else if let Some(mask) = constants.get(&lhs_id.0) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (rhs_id.0, ValueId(bid)));
                        }
                    }
                }

                // Track eq(value, and(value, MASK)) definitions.
                if let Expression::Binary {
                    operation: BinaryOperation::Eq,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    // Check both orderings: eq(value, and_result) or eq(and_result, value)
                    if let Some(&(orig_id, and_val_id)) = and_defs.get(&rhs_id.0) {
                        if orig_id == lhs_id.0 {
                            eq_mask_defs.insert(bid, (orig_id, and_val_id));
                        }
                    } else if let Some(&(orig_id, and_val_id)) = and_defs.get(&lhs_id.0) {
                        if orig_id == rhs_id.0 {
                            eq_mask_defs.insert(bid, (orig_id, and_val_id));
                        }
                    }
                }

                // Track iszero(eq_check) definitions.
                if let Expression::Unary {
                    operation: UnaryOperation::IsZero,
                    ref operand,
                } = value
                {
                    let op_id = resolve_id(operand.id, &replacements);
                    if let Some(&(orig_id, and_val_id)) = eq_mask_defs.get(&op_id.0) {
                        iszero_eq_defs.insert(bid, (orig_id, and_val_id));
                    }
                }

                // Track or(a, b) definitions so combined-check guards can
                // pull narrowing facts from any operand that is itself a gt
                // boundary check (the typical finalize_allocation pattern is
                // `if or(gt(sum, UINT64_MAX), lt(sum, addend)) { panic }`).
                if let Expression::Binary {
                    operation: BinaryOperation::Or,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    or_defs.insert(bid, (lhs_id.0, rhs_id.0));
                }

                // Track add(a, b) definitions for overflow-aware addend
                // narrowing.
                if let Expression::Binary {
                    operation: BinaryOperation::Add,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    add_defs.insert(bid, (lhs_id.0, rhs_id.0));
                }

                // Track lt(sum, addend) definitions — the canonical Solidity
                // unsigned-add-overflow check.
                if let Expression::Binary {
                    operation: BinaryOperation::Lt,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    lt_defs.insert(bid, (lhs_id.0, rhs_id.0));

                    // Additionally: if rhs is a constant K = 2^N (power of
                    // two), record lt(value, K) so a downstream
                    // `if iszero(this_lt) { panic }` can narrow value to N
                    // bits via `and(value, K-1)`. This is the canonical enum
                    // range check `if uint(value) >= EnumLen { panic }` Solidity
                    // emits as `if iszero(lt(value, EnumLen)) { panic }`.
                    if let Some(k) = constants.get(&rhs_id.0) {
                        let k_minus_one = if k.is_zero() { BigUint::ZERO } else { k - 1u32 };
                        if !k.is_zero() && (k & &k_minus_one) == BigUint::ZERO {
                            lt_const_defs.insert(
                                bid,
                                (
                                    Value {
                                        id: lhs_id,
                                        value_type: lhs.value_type,
                                    },
                                    k_minus_one,
                                ),
                            );
                        }
                    }
                }
            }

            // After tracking all binary definitions, propagate into iszero's so the
            // guard handler can recognise `if iszero(lt_const) { panic }`.
            if let Statement::Let {
                ref bindings,
                value:
                    Expression::Unary {
                        operation: UnaryOperation::IsZero,
                        ref operand,
                    },
            } = statement
            {
                if bindings.len() == 1 {
                    let bid = bindings[0].0;
                    let op_id = resolve_id(operand.id, &replacements);
                    if let Some((value, mask)) = lt_const_defs.get(&op_id.0) {
                        iszero_lt_defs.insert(bid, (*value, mask.clone()));
                    }
                }
            }
        }

        // Check for guard pattern: if gt_check { <terminates> }
        if let Statement::If {
            ref condition,
            ref then_region,
            ref else_region,
            ref outputs,
            ..
        } = statement
        {
            let cond_id = resolve_id(condition.id, &replacements);
            if else_region.is_none()
                && outputs.is_empty()
                && region_terminates(then_region, noreturn)
            {
                // Resolve the condition through any or-chains to find guard
                // checks. `if or(or(a, b), c) { panic }` implies all of a, b, c
                // are false on the fall-through; collect every gt boundary
                // check reachable through the or operands.
                let mut gt_guards: Vec<(Value, BigUint)> = Vec::new();
                let mut iszero_eq_guards: Vec<(u32, ValueId)> = Vec::new();
                // Track lt(sum, addend) operands inside the OR-chain. These
                // alone don't narrow anything, but combined with a gt(sum, MASK)
                // for the SAME sum where sum = add(a, b) and addend == a or b,
                // they prove no wraparound and we can narrow BOTH addends to
                // MASK bits as well.
                let mut lt_overflows: Vec<(u32, u32)> = Vec::new();
                let mut visit = vec![cond_id.0];
                let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
                while let Some(id) = visit.pop() {
                    if !seen.insert(id) {
                        continue;
                    }
                    if let Some((guarded, mask)) = gt_defs.get(&id) {
                        gt_guards.push((*guarded, mask.clone()));
                    } else if let Some(&(orig_id, and_val_id)) = iszero_eq_defs.get(&id) {
                        iszero_eq_guards.push((orig_id, and_val_id));
                    } else if let Some(&(sum_id, addend_id)) = lt_defs.get(&id) {
                        lt_overflows.push((sum_id, addend_id));
                    } else if let Some((value, mask)) = iszero_lt_defs.get(&id) {
                        // `if iszero(lt(value, K)) { panic }` falls through with
                        // `lt(value, K)` true, i.e., value < K. Treat the same as
                        // a gt-mask narrowing of value.
                        gt_guards.push((*value, mask.clone()));
                    } else if let Some(&(lhs_id, rhs_id)) = or_defs.get(&id) {
                        visit.push(lhs_id);
                        visit.push(rhs_id);
                    }
                }

                // Overflow-aware addend narrowing: when the OR chain proves
                // both `sum < MASK` (gt guard) and `sum >= addend` (lt
                // overflow check), the addition didn't wrap, so BOTH addends
                // individually fit in MASK bits. Solidity emits this combined
                // check for every checked addition (finalize_allocation,
                // array_allocation_size_bytes, copy_byte_array_to_storage,
                // ABI offset arithmetic, ...).
                let mut extra_gt_guards: Vec<(Value, BigUint)> = Vec::new();
                for (sum_id, addend_id) in &lt_overflows {
                    let Some(&(add_lhs, add_rhs)) = add_defs.get(sum_id) else {
                        continue;
                    };
                    // Match a gt guard on the same sum to obtain the bound.
                    let Some((_, mask)) = gt_guards.iter().find(|(g, _)| g.id.0 == *sum_id) else {
                        continue;
                    };
                    // Solidity's overflow check uses one of the addends as
                    // the lt rhs (`if lt(sum, a)`), so confirm the structural
                    // match before propagating narrowness to the OTHER addend
                    // too.
                    if *addend_id != add_lhs && *addend_id != add_rhs {
                        continue;
                    }
                    let i256_ty = Type::Int(BitWidth::I256);
                    extra_gt_guards.push((
                        Value {
                            id: ValueId(add_lhs),
                            value_type: i256_ty,
                        },
                        mask.clone(),
                    ));
                    extra_gt_guards.push((
                        Value {
                            id: ValueId(add_rhs),
                            value_type: i256_ty,
                        },
                        mask.clone(),
                    ));
                }
                gt_guards.extend(extra_gt_guards);

                // Apply gt-mask narrowing for each guarded value reachable
                // through the (possibly nested) or-chain. This catches the
                // `or(gt, lt)` pattern in finalize_allocation where the gt
                // operand bounds the sum to UINT64_MAX.
                if !gt_guards.is_empty() || !iszero_eq_guards.is_empty() {
                    new_stmts.push(statement);

                    for (guarded_val, mask) in gt_guards {
                        // Skip if this value was already narrowed by an
                        // earlier (more specific) guard in this block.
                        if replacements.contains_key(&guarded_val.id.0) {
                            continue;
                        }
                        let mask_id = ValueId(*next_id);
                        *next_id += 1;
                        let narrow_id = ValueId(*next_id);
                        *next_id += 1;

                        new_stmts.push(Statement::Let {
                            bindings: vec![mask_id],
                            value: Expression::Literal {
                                value: mask,
                                value_type: Type::Int(BitWidth::I256),
                            },
                        });
                        new_stmts.push(Statement::Let {
                            bindings: vec![narrow_id],
                            value: Expression::Binary {
                                operation: BinaryOperation::And,
                                lhs: guarded_val,
                                rhs: Value::int(mask_id),
                            },
                        });

                        replacements.insert(guarded_val.id.0, narrow_id);
                        stats.guards_narrowed += 1;
                    }

                    for (orig_id, and_val_id) in iszero_eq_guards {
                        replacements.entry(orig_id).or_insert(and_val_id);
                        stats.guards_narrowed += 1;
                    }

                    continue;
                }
            }
        }

        // Interprocedural validator narrowing: detect calls to validator functions
        // that prove their argument fits in a boundary mask. Insert AND after the
        // call to give type inference the narrowed value.
        if let Statement::Expression(Expression::Call {
            ref function,
            ref arguments,
        }) = statement
        {
            if arguments.len() == 1 {
                if let Some(mask) = validators.get(&function.0) {
                    let arg_id = resolve_id(arguments[0].id, &replacements);
                    // Don't re-narrow if already replaced (e.g., earlier validator call)
                    if let std::collections::btree_map::Entry::Vacant(e) =
                        replacements.entry(arg_id.0)
                    {
                        let mask_id = ValueId(*next_id);
                        *next_id += 1;
                        let narrow_id = ValueId(*next_id);
                        *next_id += 1;

                        // Push the validator call first.
                        new_stmts.push(statement);

                        // Emit: let mask_id = MASK
                        new_stmts.push(Statement::Let {
                            bindings: vec![mask_id],
                            value: Expression::Literal {
                                value: mask.clone(),
                                value_type: Type::Int(BitWidth::I256),
                            },
                        });

                        // Emit: let narrow_id = and(argument, mask_id)
                        new_stmts.push(Statement::Let {
                            bindings: vec![narrow_id],
                            value: Expression::Binary {
                                operation: BinaryOperation::And,
                                lhs: Value {
                                    id: arg_id,
                                    value_type: Type::Int(BitWidth::I256),
                                },
                                rhs: Value::int(mask_id),
                            },
                        });

                        // Replace subsequent uses of the argument.
                        e.insert(narrow_id);
                        stats.guards_narrowed += 1;
                        continue;
                    }
                }
            }
        }

        new_stmts.push(statement);
    }

    block.statements = new_stmts;
    replacements
}

/// Recurse into nested regions within a statement.
fn narrow_stmt_regions(
    statement: &mut Statement,
    next_id: &mut u32,
    stats: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) {
    match statement {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            narrow_region(then_region, next_id, stats, validators, noreturn);
            if let Some(r) = else_region {
                narrow_region(r, next_id, stats, validators, noreturn);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                narrow_region(&mut case.body, next_id, stats, validators, noreturn);
            }
            if let Some(d) = default {
                narrow_region(d, next_id, stats, validators, noreturn);
            }
        }
        Statement::For {
            condition_statements,
            body,
            post,
            ..
        } => {
            let mut cond_block = Block {
                statements: std::mem::take(condition_statements),
            };
            narrow_block(&mut cond_block, next_id, stats, validators, noreturn);
            *condition_statements = cond_block.statements;

            narrow_region(body, next_id, stats, validators, noreturn);
            narrow_region(post, next_id, stats, validators, noreturn);
        }
        Statement::Block(region) => {
            narrow_region(region, next_id, stats, validators, noreturn);
        }
        _ => {}
    }
}

/// Process a region as a block.
fn narrow_region(
    region: &mut Region,
    next_id: &mut u32,
    stats: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) {
    let mut block = Block {
        statements: std::mem::take(&mut region.statements),
    };
    narrow_block(&mut block, next_id, stats, validators, noreturn);
    region.statements = block.statements;
}

/// Returns true if `value` is a useful boundary mask for gt-based guard narrowing.
/// Only masks that fit in a native register width (≤64 bits) are useful for
/// the gt pattern, since we need to insert a new AND instruction.
fn is_boundary_mask(value: &BigUint) -> bool {
    if value.is_zero() {
        return false;
    }
    // Must be 2^N - 1 (all low bits set).
    let plus_one = value + 1u32;
    if (plus_one.clone() & value) != BigUint::ZERO {
        return false;
    }
    // Only useful if the mask fits in 64 bits (native register width).
    *value <= BigUint::from(u64::MAX)
}

/// Returns true if `value` is a boundary mask useful for eq-based guard narrowing.
/// For eq-based patterns (iszero(eq(value, and(value, MASK)))), we accept wider
/// masks up to 160 bits (address width). The AND already exists in the IR,
/// so we just redirect uses — no new instruction is emitted.
///
/// Narrowing from i256 to i160 is significant on a 32-bit target: 5 words
/// instead of 8, saving 37.5% register pressure per address value.
fn is_wide_boundary_mask(value: &BigUint) -> bool {
    if value.is_zero() {
        return false;
    }
    // Must be 2^N - 1 (all low bits set).
    let plus_one = value + 1u32;
    if (plus_one.clone() & value) != BigUint::ZERO {
        return false;
    }
    // Accept masks up to 160 bits (address width). Wider masks don't
    // provide enough savings to justify the narrowing overhead.
    let bits = value.bits();
    bits <= 160
}

/// Returns true if a region terminates (doesn't fall through).
/// Checks for direct terminators (revert, return, etc.) and also for
/// regions that end with a function call with no subsequent statements,
/// which indicates a call to a never-returning function (like panic helpers).
/// After the simplifier's DCE pass, dead code after unreachable calls has
/// been eliminated, so a trailing call with no yields is a reliable indicator.
fn region_terminates(region: &Region, noreturn: &std::collections::BTreeSet<u32>) -> bool {
    region.statements.iter().any(|s| {
        if matches!(
            s,
            Statement::Revert { .. }
                | Statement::Return { .. }
                | Statement::Invalid
                | Statement::Stop
                | Statement::SelfDestruct { .. }
                | Statement::Leave { .. }
                | Statement::PanicRevert { .. }
                | Statement::ErrorStringRevert { .. }
                | Statement::CustomErrorRevert { .. }
        ) {
            return true;
        }
        // A call to a known noreturn function (e.g., panic_error_0xNN) acts
        // as a terminator at this site. Without recognizing this, the OZ
        // canonical `if check { panic_error_0x41() }` pattern in
        // finalize_allocation never narrows.
        if let Statement::Expression(Expression::Call { function, .. }) = s {
            if noreturn.contains(&function.0) {
                return true;
            }
        }
        false
    })
}

/// Resolve a ValueId through the replacement chain.
fn resolve_id(id: ValueId, replacements: &BTreeMap<u32, ValueId>) -> ValueId {
    let mut current = id;
    // Follow replacement chain (at most a few hops).
    for _ in 0..8 {
        if let Some(&replacement) = replacements.get(&current.0) {
            current = replacement;
        } else {
            break;
        }
    }
    current
}

/// Replaces every used ValueId in the statement using the `replacements` map.
/// One-step lookup: if A→B is in the map, A becomes B; does not chase B→C.
/// Definitions (Let bindings, If/Switch/For outputs, loop_variables, ExternalCall/Create
/// result) are left untouched — only use sites are rewritten.
fn replace_value_ids_in_stmt(
    mut statement: Statement,
    replacements: &BTreeMap<u32, ValueId>,
) -> Statement {
    statement.for_each_value_id_mut(&mut |id| {
        if let Some(&new_id) = replacements.get(&id.0) {
            *id = new_id;
        }
    });
    statement
}
