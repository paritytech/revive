//! Guard-based value narrowing pass.
//!
//! Detects patterns like `if gt(val, MASK) { <terminates> }` and inserts
//! `let val_narrow = and(val, MASK)` after the guard, replacing all subsequent
//! uses of `val` with `val_narrow`. This gives type inference proof that the
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

    // Pre-scan: detect validator functions (void, single param, contains
    // eq-based guard pattern like `if iszero(eq(param, and(param, MASK))) { revert }`).
    // These prove their parameter fits in MASK bits, but the proof doesn't
    // propagate to callers. We record (function_id → mask) so that call sites
    // can insert narrowing ANDs.
    let validators = detect_validator_functions(object);

    let _ = narrow_block(&mut object.code, &mut next_id, &mut stats, &validators);
    for func in object.functions.values_mut() {
        let replacements = narrow_block(&mut func.body, &mut next_id, &mut stats, &validators);
        // Apply replacements to function return values. Without this, functions
        // like abi_decode_address return the unmasked value (i256) instead of
        // the AND-masked value (i160), blocking return type narrowing.
        if !replacements.is_empty() {
            for ret_val in &mut func.return_values {
                if let Some(&new_id) = replacements.get(&ret_val.0) {
                    *ret_val = new_id;
                }
            }
        }
    }

    for sub in &mut object.subobjects {
        let sub_stats = narrow_guards_in_object(sub);
        stats.guards_narrowed += sub_stats.guards_narrowed;
        stats.uses_replaced += sub_stats.uses_replaced;
    }

    stats
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
fn detect_validator_functions(object: &Object) -> BTreeMap<u32, BigUint> {
    let mut validators = BTreeMap::new();

    for (func_id, function) in &object.functions {
        // Must be void (no return values) with exactly one parameter
        if !function.returns.is_empty() || function.params.len() != 1 {
            continue;
        }

        let param_id = function.params[0].0;
        if let Some(mask) = detect_validator_mask(&function.body, param_id) {
            validators.insert(func_id.0, mask);
        }
    }

    validators
}

/// Checks if a block contains the eq-based validator pattern for the given param.
/// Returns the boundary mask if found.
fn detect_validator_mask(block: &Block, param_id: ValueId) -> Option<BigUint> {
    let stmts = &block.statements;

    // Track definitions to resolve the pattern
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    let mut and_defs: BTreeMap<u32, (u32, BigUint)> = BTreeMap::new(); // and_id -> (orig_id, mask)
    let mut eq_mask_defs: BTreeMap<u32, BigUint> = BTreeMap::new(); // eq_id -> mask
    let mut iszero_eq_defs: BTreeMap<u32, BigUint> = BTreeMap::new(); // iszero_id -> mask

    for stmt in stmts {
        if let Statement::Let {
            ref bindings,
            ref value,
        } = stmt
        {
            if bindings.len() != 1 {
                continue;
            }
            let bid = bindings[0].0;

            if let Expr::Literal {
                value: ref lit_val, ..
            } = value
            {
                constants.insert(bid, lit_val.clone());
            }

            // Track and(param, MASK)
            if let Expr::Binary {
                op: BinOp::And,
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
            if let Expr::Binary {
                op: BinOp::Eq,
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
            if let Expr::Unary {
                op: UnaryOp::IsZero,
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
        } = stmt
        {
            if else_region.is_none() && outputs.is_empty() && region_terminates(then_region) {
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
) -> BTreeMap<u32, ValueId> {
    // First, recurse into nested regions within each statement.
    for stmt in &mut block.statements {
        narrow_stmt_regions(stmt, next_id, stats, validators);
    }

    // Now process this block's top-level statements for guard patterns.
    let stmts = std::mem::take(&mut block.statements);
    let mut new_stmts = Vec::with_capacity(stmts.len() + 16);

    // Track known constants and gt-comparison definitions.
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    // gt_check_id -> (guarded_value, boundary_mask)
    let mut gt_defs: BTreeMap<u32, (Value, BigUint)> = BTreeMap::new();
    // Track and(val, MASK) definitions for eq-based patterns:
    // and_result_id -> (original_value_id, and_result_id_as_ValueId)
    let mut and_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Track eq(val, and(val, MASK)) definitions:
    // eq_result_id -> original_value_id (to be replaced with and_result after guard)
    let mut eq_mask_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Track iszero(eq_check) definitions:
    // iszero_result_id -> (original_value_id, and_result_id)
    let mut iszero_eq_defs: BTreeMap<u32, (u32, ValueId)> = BTreeMap::new();
    // Accumulated replacements: old_value_id -> new_value_id
    let mut replacements: BTreeMap<u32, ValueId> = BTreeMap::new();

    for stmt in stmts {
        // Apply accumulated replacements to this statement.
        let stmt = if replacements.is_empty() {
            stmt
        } else {
            replace_value_ids_in_stmt(stmt, &replacements)
        };

        // Track constant definitions (let vN = 0xFFFF...).
        if let Statement::Let {
            ref bindings,
            ref value,
        } = stmt
        {
            if bindings.len() == 1 {
                let bid = bindings[0].0;

                if let Expr::Literal {
                    value: ref lit_val, ..
                } = value
                {
                    constants.insert(bid, lit_val.clone());
                }

                // Track gt(val, const_boundary) definitions.
                if let Expr::Binary {
                    op: BinOp::Gt,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    // Resolve rhs through replacements and check if it's a boundary constant.
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    if let Some(mask) = constants.get(&rhs_id.0) {
                        if is_boundary_mask(mask) {
                            // gt(val, MASK): if true, val > MASK (revert); if false, val <= MASK
                            let guarded = Value {
                                id: resolve_id(lhs.id, &replacements),
                                ty: lhs.ty,
                            };
                            gt_defs.insert(bid, (guarded, mask.clone()));
                        }
                    }
                }

                // Track and(val, MASK) where MASK is a boundary mask.
                // Used for eq-based address validation: iszero(eq(val, and(val, MASK)))
                if let Expr::Binary {
                    op: BinOp::And,
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

                // Track eq(val, and(val, MASK)) definitions.
                if let Expr::Binary {
                    op: BinOp::Eq,
                    ref lhs,
                    ref rhs,
                } = value
                {
                    let lhs_id = resolve_id(lhs.id, &replacements);
                    let rhs_id = resolve_id(rhs.id, &replacements);
                    // Check both orderings: eq(val, and_result) or eq(and_result, val)
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
                if let Expr::Unary {
                    op: UnaryOp::IsZero,
                    ref operand,
                } = value
                {
                    let op_id = resolve_id(operand.id, &replacements);
                    if let Some(&(orig_id, and_val_id)) = eq_mask_defs.get(&op_id.0) {
                        iszero_eq_defs.insert(bid, (orig_id, and_val_id));
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
        } = stmt
        {
            let cond_id = resolve_id(condition.id, &replacements);
            if else_region.is_none() && outputs.is_empty() && region_terminates(then_region) {
                if let Some((guarded_val, mask)) = gt_defs.get(&cond_id.0).cloned() {
                    // Found: if gt(val, MASK) { <terminates> }
                    // After this if, val <= MASK.
                    // Insert: let val_narrow = and(val, MASK)
                    // Replace subsequent uses of val with val_narrow.

                    let mask_id = ValueId(*next_id);
                    *next_id += 1;
                    let narrow_id = ValueId(*next_id);
                    *next_id += 1;

                    // Push the if statement first.
                    new_stmts.push(stmt);

                    // Emit: let mask_id = MASK
                    new_stmts.push(Statement::Let {
                        bindings: vec![mask_id],
                        value: Expr::Literal {
                            value: mask,
                            ty: Type::Int(BitWidth::I256),
                        },
                    });

                    // Emit: let narrow_id = and(val, mask_id)
                    new_stmts.push(Statement::Let {
                        bindings: vec![narrow_id],
                        value: Expr::Binary {
                            op: BinOp::And,
                            lhs: guarded_val,
                            rhs: Value::int(mask_id),
                        },
                    });

                    // Replace subsequent uses of the guarded value.
                    replacements.insert(guarded_val.id.0, narrow_id);
                    stats.guards_narrowed += 1;
                    continue;
                }

                // Check for eq-based guard pattern:
                // let masked = and(val, MASK)
                // let eq_check = eq(val, masked)
                // let not_check = iszero(eq_check)
                // if not_check { <terminates> }
                // After: val == masked, so val fits in MASK bits.
                // Replace subsequent uses of val with masked.
                if let Some(&(orig_id, and_val_id)) = iszero_eq_defs.get(&cond_id.0) {
                    // Push the if statement first.
                    new_stmts.push(stmt);

                    // Replace subsequent uses of the original value with the
                    // AND-masked value. No new instructions needed since the
                    // and(val, MASK) already exists.
                    replacements.insert(orig_id, and_val_id);
                    stats.guards_narrowed += 1;
                    continue;
                }
            }
        }

        // Interprocedural validator narrowing: detect calls to validator functions
        // that prove their argument fits in a boundary mask. Insert AND after the
        // call to give type inference the narrowed value.
        if let Statement::Expr(Expr::Call {
            ref function,
            ref args,
        }) = stmt
        {
            if args.len() == 1 {
                if let Some(mask) = validators.get(&function.0) {
                    let arg_id = resolve_id(args[0].id, &replacements);
                    // Don't re-narrow if already replaced (e.g., earlier validator call)
                    if let std::collections::btree_map::Entry::Vacant(e) =
                        replacements.entry(arg_id.0)
                    {
                        let mask_id = ValueId(*next_id);
                        *next_id += 1;
                        let narrow_id = ValueId(*next_id);
                        *next_id += 1;

                        // Push the validator call first.
                        new_stmts.push(stmt);

                        // Emit: let mask_id = MASK
                        new_stmts.push(Statement::Let {
                            bindings: vec![mask_id],
                            value: Expr::Literal {
                                value: mask.clone(),
                                ty: Type::Int(BitWidth::I256),
                            },
                        });

                        // Emit: let narrow_id = and(arg, mask_id)
                        new_stmts.push(Statement::Let {
                            bindings: vec![narrow_id],
                            value: Expr::Binary {
                                op: BinOp::And,
                                lhs: Value {
                                    id: arg_id,
                                    ty: Type::Int(BitWidth::I256),
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

        new_stmts.push(stmt);
    }

    block.statements = new_stmts;
    replacements
}

/// Recurse into nested regions within a statement.
fn narrow_stmt_regions(
    stmt: &mut Statement,
    next_id: &mut u32,
    stats: &mut GuardNarrowStats,
    validators: &BTreeMap<u32, BigUint>,
) {
    match stmt {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            narrow_region(then_region, next_id, stats, validators);
            if let Some(r) = else_region {
                narrow_region(r, next_id, stats, validators);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                narrow_region(&mut case.body, next_id, stats, validators);
            }
            if let Some(d) = default {
                narrow_region(d, next_id, stats, validators);
            }
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            let mut cond_block = Block {
                statements: std::mem::take(condition_stmts),
            };
            narrow_block(&mut cond_block, next_id, stats, validators);
            *condition_stmts = cond_block.statements;

            narrow_region(body, next_id, stats, validators);
            narrow_region(post, next_id, stats, validators);
        }
        Statement::Block(region) => {
            narrow_region(region, next_id, stats, validators);
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
) {
    let mut block = Block {
        statements: std::mem::take(&mut region.statements),
    };
    narrow_block(&mut block, next_id, stats, validators);
    region.statements = block.statements;
}

/// Returns true if `val` is a useful boundary mask for gt-based guard narrowing.
/// Only masks that fit in a native register width (≤64 bits) are useful for
/// the gt pattern, since we need to insert a new AND instruction.
fn is_boundary_mask(val: &BigUint) -> bool {
    if val.is_zero() {
        return false;
    }
    // Must be 2^N - 1 (all low bits set).
    let plus_one = val + 1u32;
    if (plus_one.clone() & val) != BigUint::ZERO {
        return false;
    }
    // Only useful if the mask fits in 64 bits (native register width).
    *val <= BigUint::from(u64::MAX)
}

/// Returns true if `val` is a boundary mask useful for eq-based guard narrowing.
/// For eq-based patterns (iszero(eq(val, and(val, MASK)))), we accept wider
/// masks up to 160 bits (address width). The AND already exists in the IR,
/// so we just redirect uses — no new instruction is emitted.
///
/// Narrowing from i256 to i160 is significant on a 32-bit target: 5 words
/// instead of 8, saving 37.5% register pressure per address value.
fn is_wide_boundary_mask(val: &BigUint) -> bool {
    if val.is_zero() {
        return false;
    }
    // Must be 2^N - 1 (all low bits set).
    let plus_one = val + 1u32;
    if (plus_one.clone() & val) != BigUint::ZERO {
        return false;
    }
    // Accept masks up to 160 bits (address width). Wider masks don't
    // provide enough savings to justify the narrowing overhead.
    let bits = val.bits();
    bits <= 160
}

/// Returns true if a region terminates (doesn't fall through).
/// Checks for direct terminators (revert, return, etc.) and also for
/// regions that end with a function call with no subsequent statements,
/// which indicates a call to a never-returning function (like panic helpers).
/// After the simplifier's DCE pass, dead code after unreachable calls has
/// been eliminated, so a trailing call with no yields is a reliable indicator.
fn region_terminates(region: &Region) -> bool {
    region.statements.iter().any(|s| {
        matches!(
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
        )
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
/// Definitions (Let bindings, If/Switch/For outputs, loop_vars, ExternalCall/Create
/// result) are left untouched — only use sites are rewritten.
fn replace_value_ids_in_stmt(
    mut stmt: Statement,
    replacements: &BTreeMap<u32, ValueId>,
) -> Statement {
    stmt.for_each_value_id_mut(&mut |id| {
        if let Some(&new_id) = replacements.get(&id.0) {
            *id = new_id;
        }
    });
    stmt
}
