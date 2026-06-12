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

impl std::ops::AddAssign for GuardNarrowStats {
    fn add_assign(&mut self, rhs: Self) {
        self.guards_narrowed += rhs.guards_narrowed;
        self.uses_replaced += rhs.uses_replaced;
    }
}

/// Runs guard narrowing on an entire object tree (including subobjects).
pub fn narrow_guards_in_object(object: &mut Object) -> GuardNarrowStats {
    let mut next_id = object.find_max_value_id() + 1;
    let mut statistics = GuardNarrowStats::default();

    let noreturn = detect_noreturn_functions(object);

    let validators = detect_validator_functions(object, &noreturn);

    let _ = narrow_block(
        &mut object.code,
        &mut next_id,
        &mut statistics,
        &validators,
        &noreturn,
    );
    for function in object.functions.values_mut() {
        let replacements = narrow_block(
            &mut function.body,
            &mut next_id,
            &mut statistics,
            &validators,
            &noreturn,
        );
        if !replacements.is_empty() {
            for return_value in &mut function.return_values {
                if let Some(&new_id) = replacements.get(&return_value.0) {
                    *return_value = new_id;
                }
            }
        }
    }

    narrow_allocator_param_types(object, &noreturn);

    for subobject in &mut object.subobjects {
        statistics += narrow_guards_in_object(subobject);
    }

    statistics
}

/// Narrows the free-memory-pointer parameter (index 0) of allocator-shaped functions from
/// i256 to i64. In solc's `finalize_allocation(memPtr, size)` this is `memPtr`: it is bounded
/// by the heap size, and the function's own `or(gt(sum, UINT64_MAX), lt(sum, p0))` guard also
/// reverts (matching EVM) for any pointer wide enough to overflow the new free pointer, so the
/// call-site truncation is sound.
///
/// The size parameter (index 1) is deliberately left i256. Narrowing it would require asserting
/// `size <= UINT64_MAX`, which is unsound: for `size ∈ [2^256-31, 2^256-1]` the rounding
/// `and(add(size, 31), not(31))` wraps to 0, so EVM leaves the free pointer unchanged and does
/// not revert — but the assertion would trap. See [`is_allocator_function`].
fn narrow_allocator_param_types(object: &mut Object, noreturn: &std::collections::BTreeSet<u32>) {
    let mut narrow_params: BTreeMap<u32, Vec<usize>> = BTreeMap::new();

    for (function_id, function) in &object.functions {
        if is_allocator_function(function, noreturn) {
            narrow_params.insert(function_id.0, vec![0]);
        }
    }

    for (function_id, indices) in narrow_params {
        if let Some(function) = object.functions.get_mut(&FunctionId(function_id)) {
            for i in indices {
                if let Some((_, value_type)) = function.parameters.get_mut(i) {
                    *value_type = Type::Int(BitWidth::I64);
                }
            }
        }
    }
}

/// Returns whether `function` has the allocator shape that guard narrowing
/// specializes: a two-parameter `(i256, i256) -> ()` function whose body matches
/// [`has_allocator_shape`]. [`narrow_allocator_param_types`] keys off this.
fn is_allocator_function(function: &Function, noreturn: &std::collections::BTreeSet<u32>) -> bool {
    function.parameters.len() == 2
        && function.returns.is_empty()
        && function
            .parameters
            .iter()
            .all(|(_, value_type)| matches!(value_type, Type::Int(BitWidth::I256)))
        && has_allocator_shape(&function.body, function.parameters[0].0, noreturn)
}

/// Detects the canonical allocator pattern in a block:
/// * `mstore(0x40, sum)` (FMP store)
/// * `sum = add(p0, x)` somewhere upstream
/// * `if or(gt(sum, UINT64_MAX), lt(sum, p0)) { <reverts> }` overflow check
/// * `x` is derived from `p1` (we don't constrain the alignment computation
///   precisely; the FMP store + add(p0, _) + overflow check is enough).
///
/// The overflow check must actually feed an aborting `if` that **reverts** (panics) — not merely
/// exist as dead comparisons, and not merely `leave`/`return`. That guard is what makes narrowing
/// `p0` sound: a caller passing a wide pointer reverts under EVM exactly as the narrowed call-site
/// truncation traps. Without it, a function that computes-but-ignores the checks would be narrowed
/// and trap on inputs EVM accepts.
fn has_allocator_shape(
    block: &Block,
    p0: ValueId,
    noreturn: &std::collections::BTreeSet<u32>,
) -> bool {
    let mut add_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut and_results: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new();
    let mut or_results: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut gt_const_checks: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new();
    let mut lt_against_addend: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut fmp_store_value: Option<u32> = None;
    // Conditions of `if` statements whose then-region reverts (aborts the transaction).
    let mut reverting_if_conditions: std::collections::BTreeSet<u32> =
        std::collections::BTreeSet::new();

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
                            if let Some(mask) = constants.get(&rhs.id.0) {
                                and_results.insert(bid, (lhs.id.0, mask.clone()));
                            } else if let Some(mask) = constants.get(&lhs.id.0) {
                                and_results.insert(bid, (rhs.id.0, mask.clone()));
                            }
                        }
                        BinaryOperation::Or => {
                            or_results.insert(bid, (lhs.id.0, rhs.id.0));
                        }
                        BinaryOperation::Gt => {
                            if let Some(c) = constants.get(&rhs.id.0) {
                                gt_const_checks.insert(bid, (lhs.id.0, c.clone()));
                            }
                        }
                        BinaryOperation::Lt => {
                            lt_against_addend.insert(bid, (lhs.id.0, rhs.id.0));
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
        if let Statement::If {
            condition,
            then_region,
            ..
        } = statement
        {
            if region_reverts(then_region, noreturn) {
                reverting_if_conditions.insert(condition.id.0);
            }
        }
    }

    let Some(fmp_value) = fmp_store_value else {
        return false;
    };

    let sum_id = match and_results.get(&fmp_value) {
        Some((orig, _mask)) => *orig,
        None => fmp_value,
    };

    let Some(&(add_lhs, add_rhs)) = add_results.get(&sum_id) else {
        return false;
    };
    let p0_used = add_lhs == p0.0 || add_rhs == p0.0;
    if !p0_used {
        return false;
    }

    // Collect the specific comparison bindings `gt(sum, UINT64_MAX)` and `lt(sum, p0)`.
    let uint64_max: BigUint = (BigUint::from(1u32) << 64) - 1u32;
    let gt_ids: std::collections::BTreeSet<u32> = gt_const_checks
        .iter()
        .filter(|(_, (value, c))| *value == sum_id && *c == uint64_max)
        .map(|(bid, _)| *bid)
        .collect();
    let lt_ids: std::collections::BTreeSet<u32> = lt_against_addend
        .iter()
        .filter(|(_, (sum, addend))| *sum == sum_id && *addend == p0.0)
        .map(|(bid, _)| *bid)
        .collect();
    if gt_ids.is_empty() || lt_ids.is_empty() {
        return false;
    }

    // Require an aborting `if or(gt, lt) { revert }` over those two comparisons. This is what
    // guarantees the function reverts for a pointer wide enough to overflow `sum`, making the
    // `p0` narrowing sound.
    or_results.iter().any(|(or_id, (a, b))| {
        reverting_if_conditions.contains(or_id)
            && ((gt_ids.contains(a) && lt_ids.contains(b))
                || (gt_ids.contains(b) && lt_ids.contains(a)))
    })
}

/// Whether the region unconditionally aborts the transaction (reverts or panics), as opposed to
/// merely returning from the current function (`leave`) or halting successfully (`return`/`stop`).
/// Stricter than [`region_terminates`]: it is used to confirm an allocator's overflow guard
/// actually reverts on a wide value, so narrowing its pointer parameter matches EVM.
fn region_reverts(region: &Region, noreturn: &std::collections::BTreeSet<u32>) -> bool {
    region.statements.iter().any(|s| {
        if matches!(
            s,
            Statement::Revert { .. }
                | Statement::Invalid
                | Statement::PanicRevert { .. }
                | Statement::ErrorStringRevert { .. }
                | Statement::CustomErrorRevert { .. }
        ) {
            return true;
        }
        if let Statement::Expression(Expression::Call { function, .. }) = s {
            if noreturn.contains(&function.0) {
                return true;
            }
        }
        false
    })
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
        for (function_id, function) in &object.functions {
            if noreturn.contains(&function_id.0) || function.body.statements.len() != 1 {
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
                noreturn.insert(function_id.0);
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

    for (function_id, function) in &object.functions {
        if !function.returns.is_empty() || function.parameters.len() != 1 {
            continue;
        }

        let parameter_id = function.parameters[0].0;
        if let Some(mask) = detect_validator_mask(&function.body, parameter_id, noreturn) {
            validators.insert(function_id.0, mask);
        }
    }

    validators
}

/// Checks if a block contains the eq-based validator pattern for the given param.
/// Returns the boundary mask if found.
fn detect_validator_mask(
    block: &Block,
    parameter_id: ValueId,
    noreturn: &std::collections::BTreeSet<u32>,
) -> Option<BigUint> {
    let statements = &block.statements;

    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut and_defs: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new();
    let mut eq_mask_defs: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut iszero_eq_defs: BTreeMap<u32, BigUint> = BTreeMap::new();

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

            if let Expression::Binary {
                operation: BinaryOperation::And,
                ref lhs,
                ref rhs,
            } = value
            {
                let (val_id, mask_id) = (lhs.id.0, rhs.id.0);
                if val_id == parameter_id.0 {
                    if let Some(mask) = constants.get(&mask_id) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (val_id, mask.clone()));
                        }
                    }
                } else if mask_id == parameter_id.0 {
                    if let Some(mask) = constants.get(&val_id) {
                        if is_wide_boundary_mask(mask) {
                            and_defs.insert(bid, (mask_id, mask.clone()));
                        }
                    }
                }
            }

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
    statistics: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) -> BTreeMap<u32, ValueId> {
    for statement in &mut block.statements {
        narrow_stmt_regions(statement, next_id, statistics, validators, noreturn);
    }

    let statements = std::mem::take(&mut block.statements);
    let mut new_stmts = Vec::with_capacity(statements.len() + 16);

    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut gt_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    let mut and_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    let mut eq_mask_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    let mut iszero_eq_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    let mut or_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut add_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut lt_defs: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut lt_const_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    let mut iszero_lt_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    let mut replacements: BTreeMap<u32, ValueId> = BTreeMap::new();

    for statement in statements {
        let statement = if replacements.is_empty() {
            statement
        } else {
            replace_value_ids_in_statement(statement, &replacements)
        };

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

                if let Expression::Binary {
                    operation: BinaryOperation::Gt,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    if let Some(mask) = constants.get(&rhs_id.0) {
                        if is_boundary_mask(mask) {
                            let guarded = Value {
                                id: resolve_id(lhs.id, &replacements),
                                value_type: lhs.value_type,
                            };
                            gt_defs.insert(bid, (guarded, mask.clone()));
                        }
                    }
                }

                if let Expression::Binary {
                    operation: BinaryOperation::And,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
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

                if let Expression::Binary {
                    operation: BinaryOperation::Eq,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
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

                if let Expression::Binary {
                    operation: BinaryOperation::Lt,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    lt_defs.insert(bid, (lhs_id.0, rhs_id.0));

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
                let mut gt_guards: Vec<(Value, BigUint)> = Vec::new();
                let mut iszero_eq_guards: Vec<(u32, ValueId)> = Vec::new();
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
                        gt_guards.push((*value, mask.clone()));
                    } else if let Some(&(lhs_id, rhs_id)) = or_defs.get(&id) {
                        visit.push(lhs_id);
                        visit.push(rhs_id);
                    }
                }

                let mut extra_gt_guards: Vec<(Value, BigUint)> = Vec::new();
                for (sum_id, addend_id) in &lt_overflows {
                    let Some(&(add_lhs, add_rhs)) = add_defs.get(sum_id) else {
                        continue;
                    };
                    let Some((_, mask)) = gt_guards.iter().find(|(g, _)| g.id.0 == *sum_id) else {
                        continue;
                    };
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

                if !gt_guards.is_empty() || !iszero_eq_guards.is_empty() {
                    new_stmts.push(statement);

                    for (guarded_val, mask) in gt_guards {
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
                        statistics.guards_narrowed += 1;
                    }

                    for (orig_id, and_val_id) in iszero_eq_guards {
                        replacements.entry(orig_id).or_insert(and_val_id);
                        statistics.guards_narrowed += 1;
                    }

                    continue;
                }
            }
        }

        if let Statement::Expression(Expression::Call {
            ref function,
            ref arguments,
        }) = statement
        {
            if arguments.len() == 1 {
                if let Some(mask) = validators.get(&function.0) {
                    let arg_id = resolve_id(arguments[0].id, &replacements);
                    if let std::collections::btree_map::Entry::Vacant(e) =
                        replacements.entry(arg_id.0)
                    {
                        let mask_id = ValueId(*next_id);
                        *next_id += 1;
                        let narrow_id = ValueId(*next_id);
                        *next_id += 1;

                        new_stmts.push(statement);

                        new_stmts.push(Statement::Let {
                            bindings: vec![mask_id],
                            value: Expression::Literal {
                                value: mask.clone(),
                                value_type: Type::Int(BitWidth::I256),
                            },
                        });

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

                        e.insert(narrow_id);
                        statistics.guards_narrowed += 1;
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
    statistics: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) {
    match statement {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            narrow_region(then_region, next_id, statistics, validators, noreturn);
            if let Some(r) = else_region {
                narrow_region(r, next_id, statistics, validators, noreturn);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                narrow_region(&mut case.body, next_id, statistics, validators, noreturn);
            }
            if let Some(d) = default {
                narrow_region(d, next_id, statistics, validators, noreturn);
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
            narrow_block(&mut cond_block, next_id, statistics, validators, noreturn);
            *condition_statements = cond_block.statements;

            narrow_region(body, next_id, statistics, validators, noreturn);
            narrow_region(post, next_id, statistics, validators, noreturn);
        }
        Statement::Block(region) => {
            narrow_region(region, next_id, statistics, validators, noreturn);
        }
        _ => {}
    }
}

/// Process a region as a block.
fn narrow_region(
    region: &mut Region,
    next_id: &mut u32,
    statistics: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
    noreturn: &std::collections::BTreeSet<u32>,
) {
    let mut block = Block {
        statements: std::mem::take(&mut region.statements),
    };
    narrow_block(&mut block, next_id, statistics, validators, noreturn);
    region.statements = block.statements;
}

/// Returns true if `value` is a useful boundary mask for gt-based guard narrowing.
/// Only masks that fit in a native register width (≤64 bits) are useful for
/// the gt pattern, since we need to insert a new AND instruction.
fn is_boundary_mask(value: &BigUint) -> bool {
    if value.is_zero() {
        return false;
    }
    let plus_one = value + 1u32;
    if (plus_one.clone() & value) != BigUint::ZERO {
        return false;
    }
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
    let plus_one = value + 1u32;
    if (plus_one.clone() & value) != BigUint::ZERO {
        return false;
    }
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
fn replace_value_ids_in_statement(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Builds a `finalize_allocation(memPtr, size)`-shaped function (param 0 = `memPtr`/`p0`,
    /// param 1 = `size`/`p1`). When `with_guard` is set, the overflow comparisons feed an aborting
    /// `if or(gt(sum, u64max), lt(sum, p0)) { panic }`; otherwise they are computed but ignored.
    fn allocator_object(with_guard: bool) -> Object {
        let p0 = ValueId(0);
        let p1 = ValueId(1);
        let c40 = ValueId(2);
        let umax = ValueId(3);
        let c31 = ValueId(4);
        let not31 = ValueId(5);
        let rounded = ValueId(6);
        let round_up = ValueId(7);
        let sum = ValueId(8);
        let gt = ValueId(9);
        let lt = ValueId(10);
        let or_check = ValueId(11);
        let u64_max: BigUint = (BigUint::from(1u32) << 64) - 1u32;
        let not_31: BigUint = (BigUint::from(1u32) << 256) - BigUint::from(32u32);

        // Mirror solc's `finalize_allocation`: the size is rounded up to a word before being added
        // to the pointer, so `p1` sits behind `add`+`and` rather than feeding the guard directly.
        let mut statements = vec![
            Statement::Let {
                bindings: vec![c40],
                value: Expression::Literal {
                    value: BigUint::from(0x40u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![umax],
                value: Expression::Literal {
                    value: u64_max,
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![c31],
                value: Expression::Literal {
                    value: BigUint::from(31u32),
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![not31],
                value: Expression::Literal {
                    value: not_31,
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: vec![rounded],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(p1),
                    rhs: Value::int(c31),
                },
            },
            Statement::Let {
                bindings: vec![round_up],
                value: Expression::Binary {
                    operation: BinaryOperation::And,
                    lhs: Value::int(rounded),
                    rhs: Value::int(not31),
                },
            },
            Statement::Let {
                bindings: vec![sum],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
                    lhs: Value::int(p0),
                    rhs: Value::int(round_up),
                },
            },
            Statement::Let {
                bindings: vec![gt],
                value: Expression::Binary {
                    operation: BinaryOperation::Gt,
                    lhs: Value::int(sum),
                    rhs: Value::int(umax),
                },
            },
            Statement::Let {
                bindings: vec![lt],
                value: Expression::Binary {
                    operation: BinaryOperation::Lt,
                    lhs: Value::int(sum),
                    rhs: Value::int(p0),
                },
            },
        ];
        if with_guard {
            statements.push(Statement::Let {
                bindings: vec![or_check],
                value: Expression::Binary {
                    operation: BinaryOperation::Or,
                    lhs: Value::int(gt),
                    rhs: Value::int(lt),
                },
            });
            statements.push(Statement::If {
                condition: Value::new(or_check, Type::Int(BitWidth::I1)),
                inputs: Vec::new(),
                then_region: Region {
                    statements: vec![Statement::PanicRevert { code: 0x41 }],
                    yields: Vec::new(),
                },
                else_region: None,
                outputs: Vec::new(),
            });
        }
        statements.push(Statement::MStore {
            offset: Value::int(c40),
            value: Value::int(sum),
            region: MemoryRegion::Unknown,
        });

        let mut function = Function::new(FunctionId(0), "finalize_allocation".to_string());
        function.parameters = vec![
            (p0, Type::Int(BitWidth::I256)),
            (p1, Type::Int(BitWidth::I256)),
        ];
        function.body = Block { statements };

        let mut functions = BTreeMap::new();
        functions.insert(FunctionId(0), function);
        Object {
            name: "test".to_string(),
            code: Block { statements: vec![] },
            functions,
            subobjects: vec![],
            data: BTreeMap::new(),
        }
    }

    /// True if any top-level `Let` binds `and(target, _)` / `and(_, target)` — the artifact of the
    /// (removed) unsound size-parameter mask.
    fn body_masks(statements: &[Statement], target: ValueId) -> bool {
        statements.iter().any(|s| {
            matches!(
                s,
                Statement::Let {
                    value: Expression::Binary { operation: BinaryOperation::And, lhs, rhs },
                    ..
                } if lhs.id == target || rhs.id == target
            )
        })
    }

    /// A guarded allocator: the heap-bounded pointer `p0` narrows to i64, but the size `p1` stays
    /// i256 — narrowing it would be unsound in the alignment-wrap window — and gains no mask/trap.
    #[test]
    fn guarded_allocator_narrows_pointer_not_size() {
        let mut object = allocator_object(true);
        narrow_guards_in_object(&mut object);

        let function = &object.functions[&FunctionId(0)];
        assert_eq!(
            function.parameters[0].1,
            Type::Int(BitWidth::I64),
            "the heap-bounded pointer parameter should narrow"
        );
        assert_eq!(
            function.parameters[1].1,
            Type::Int(BitWidth::I256),
            "the size parameter must stay i256"
        );
        assert!(
            !body_masks(&function.body.statements, ValueId(1)),
            "the size parameter must not be masked/trapped"
        );
    }

    /// N9: overflow comparisons that are computed but never feed an aborting `if` are not the
    /// allocator shape, so the pointer parameter must not be narrowed — narrowing it would make
    /// the call-site truncation trap on wide pointers that EVM accepts.
    #[test]
    fn non_aborting_overflow_check_is_not_an_allocator() {
        let mut object = allocator_object(false);
        narrow_guards_in_object(&mut object);

        let function = &object.functions[&FunctionId(0)];
        assert_eq!(
            function.parameters[0].1,
            Type::Int(BitWidth::I256),
            "without an aborting overflow guard the pointer must not be narrowed"
        );
        assert_eq!(function.parameters[1].1, Type::Int(BitWidth::I256));
    }
}
