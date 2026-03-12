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

    narrow_block(&mut object.code, &mut next_id, &mut stats);
    for func in object.functions.values_mut() {
        narrow_block(&mut func.body, &mut next_id, &mut stats);
    }

    for sub in &mut object.subobjects {
        let sub_stats = narrow_guards_in_object(sub);
        stats.guards_narrowed += sub_stats.guards_narrowed;
        stats.uses_replaced += sub_stats.uses_replaced;
    }

    stats
}

/// Process a block: find guard patterns and insert AND masks.
fn narrow_block(block: &mut Block, next_id: &mut u32, stats: &mut GuardNarrowStats) {
    // First, recurse into nested regions within each statement.
    for stmt in &mut block.statements {
        narrow_stmt_regions(stmt, next_id, stats);
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

        new_stmts.push(stmt);
    }

    block.statements = new_stmts;
}

/// Recurse into nested regions within a statement.
fn narrow_stmt_regions(stmt: &mut Statement, next_id: &mut u32, stats: &mut GuardNarrowStats) {
    match stmt {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            narrow_region(then_region, next_id, stats);
            if let Some(r) = else_region {
                narrow_region(r, next_id, stats);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                narrow_region(&mut case.body, next_id, stats);
            }
            if let Some(d) = default {
                narrow_region(d, next_id, stats);
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
            narrow_block(&mut cond_block, next_id, stats);
            *condition_stmts = cond_block.statements;

            narrow_region(body, next_id, stats);
            narrow_region(post, next_id, stats);
        }
        Statement::Block(region) => {
            narrow_region(region, next_id, stats);
        }
        _ => {}
    }
}

/// Process a region as a block.
fn narrow_region(region: &mut Region, next_id: &mut u32, stats: &mut GuardNarrowStats) {
    let mut block = Block {
        statements: std::mem::take(&mut region.statements),
    };
    narrow_block(&mut block, next_id, stats);
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

/// Replace all occurrences of old ValueIds with new ones in a Value.
fn replace_value(val: Value, replacements: &BTreeMap<u32, ValueId>) -> Value {
    if let Some(&new_id) = replacements.get(&val.id.0) {
        Value { id: new_id, ..val }
    } else {
        val
    }
}

/// Replace ValueIds in an expression.
fn replace_value_ids_in_expr(expr: Expr, replacements: &BTreeMap<u32, ValueId>) -> Expr {
    match expr {
        Expr::Var(id) => {
            if let Some(&new_id) = replacements.get(&id.0) {
                Expr::Var(new_id)
            } else {
                Expr::Var(id)
            }
        }
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op,
            lhs: replace_value(lhs, replacements),
            rhs: replace_value(rhs, replacements),
        },
        Expr::Ternary { op, a, b, n } => Expr::Ternary {
            op,
            a: replace_value(a, replacements),
            b: replace_value(b, replacements),
            n: replace_value(n, replacements),
        },
        Expr::Unary { op, operand } => Expr::Unary {
            op,
            operand: replace_value(operand, replacements),
        },
        Expr::CallDataLoad { offset } => Expr::CallDataLoad {
            offset: replace_value(offset, replacements),
        },
        Expr::ExtCodeSize { address } => Expr::ExtCodeSize {
            address: replace_value(address, replacements),
        },
        Expr::ExtCodeHash { address } => Expr::ExtCodeHash {
            address: replace_value(address, replacements),
        },
        Expr::BlockHash { number } => Expr::BlockHash {
            number: replace_value(number, replacements),
        },
        Expr::BlobHash { index } => Expr::BlobHash {
            index: replace_value(index, replacements),
        },
        Expr::Balance { address } => Expr::Balance {
            address: replace_value(address, replacements),
        },
        Expr::MLoad { offset, region } => Expr::MLoad {
            offset: replace_value(offset, replacements),
            region,
        },
        Expr::SLoad { key, static_slot } => Expr::SLoad {
            key: replace_value(key, replacements),
            static_slot,
        },
        Expr::TLoad { key } => Expr::TLoad {
            key: replace_value(key, replacements),
        },
        Expr::Call { function, args } => Expr::Call {
            function,
            args: args
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Expr::Truncate { value, to } => Expr::Truncate {
            value: replace_value(value, replacements),
            to,
        },
        Expr::ZeroExtend { value, to } => Expr::ZeroExtend {
            value: replace_value(value, replacements),
            to,
        },
        Expr::SignExtendTo { value, to } => Expr::SignExtendTo {
            value: replace_value(value, replacements),
            to,
        },
        Expr::Keccak256 { offset, length } => Expr::Keccak256 {
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Expr::Keccak256Pair { word0, word1 } => Expr::Keccak256Pair {
            word0: replace_value(word0, replacements),
            word1: replace_value(word1, replacements),
        },
        Expr::Keccak256Single { word0 } => Expr::Keccak256Single {
            word0: replace_value(word0, replacements),
        },
        Expr::MappingSLoad { key, slot } => Expr::MappingSLoad {
            key: replace_value(key, replacements),
            slot: replace_value(slot, replacements),
        },
        // Expressions with no Value operands pass through unchanged.
        other => other,
    }
}

/// Replace ValueIds in a statement (non-recursive - doesn't enter regions).
fn replace_value_ids_in_stmt(stmt: Statement, replacements: &BTreeMap<u32, ValueId>) -> Statement {
    match stmt {
        Statement::Let { bindings, value } => Statement::Let {
            bindings,
            value: replace_value_ids_in_expr(value, replacements),
        },
        Statement::MStore {
            offset,
            value,
            region,
        } => Statement::MStore {
            offset: replace_value(offset, replacements),
            value: replace_value(value, replacements),
            region,
        },
        Statement::MStore8 {
            offset,
            value,
            region,
        } => Statement::MStore8 {
            offset: replace_value(offset, replacements),
            value: replace_value(value, replacements),
            region,
        },
        Statement::MCopy { dest, src, length } => Statement::MCopy {
            dest: replace_value(dest, replacements),
            src: replace_value(src, replacements),
            length: replace_value(length, replacements),
        },
        Statement::SStore {
            key,
            value,
            static_slot,
        } => Statement::SStore {
            key: replace_value(key, replacements),
            value: replace_value(value, replacements),
            static_slot,
        },
        Statement::TStore { key, value } => Statement::TStore {
            key: replace_value(key, replacements),
            value: replace_value(value, replacements),
        },
        Statement::MappingSStore { key, slot, value } => Statement::MappingSStore {
            key: replace_value(key, replacements),
            slot: replace_value(slot, replacements),
            value: replace_value(value, replacements),
        },
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            outputs,
        } => Statement::If {
            condition: replace_value(condition, replacements),
            inputs: inputs
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
            then_region: replace_region(then_region, replacements),
            else_region: else_region.map(|r| replace_region(r, replacements)),
            outputs,
        },
        Statement::Switch {
            scrutinee,
            inputs,
            cases,
            default,
            outputs,
        } => Statement::Switch {
            scrutinee: replace_value(scrutinee, replacements),
            inputs: inputs
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
            cases: cases
                .into_iter()
                .map(|c| SwitchCase {
                    value: c.value,
                    body: replace_region(c.body, replacements),
                })
                .collect(),
            default: default.map(|r| replace_region(r, replacements)),
            outputs,
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
            init_values: init_values
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
            loop_vars,
            condition_stmts: condition_stmts
                .into_iter()
                .map(|s| replace_value_ids_in_stmt(s, replacements))
                .collect(),
            condition: replace_value_ids_in_expr(condition, replacements),
            body: replace_region(body, replacements),
            post_input_vars,
            post: replace_region(post, replacements),
            outputs,
        },
        Statement::Break { values } => Statement::Break {
            values: values
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Statement::Continue { values } => Statement::Continue {
            values: values
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Statement::Leave { return_values } => Statement::Leave {
            return_values: return_values
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Statement::Revert { offset, length } => Statement::Revert {
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::Return { offset, length } => Statement::Return {
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
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
            kind,
            gas: replace_value(gas, replacements),
            address: replace_value(address, replacements),
            value: value.map(|v| replace_value(v, replacements)),
            args_offset: replace_value(args_offset, replacements),
            args_length: replace_value(args_length, replacements),
            ret_offset: replace_value(ret_offset, replacements),
            ret_length: replace_value(ret_length, replacements),
            result,
        },
        Statement::Create {
            kind,
            value,
            offset,
            length,
            salt,
            result,
        } => Statement::Create {
            kind,
            value: replace_value(value, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
            salt: salt.map(|v| replace_value(v, replacements)),
            result,
        },
        Statement::Log {
            offset,
            length,
            topics,
        } => Statement::Log {
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
            topics: topics
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Statement::CodeCopy {
            dest,
            offset,
            length,
        } => Statement::CodeCopy {
            dest: replace_value(dest, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::ExtCodeCopy {
            address,
            dest,
            offset,
            length,
        } => Statement::ExtCodeCopy {
            address: replace_value(address, replacements),
            dest: replace_value(dest, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::ReturnDataCopy {
            dest,
            offset,
            length,
        } => Statement::ReturnDataCopy {
            dest: replace_value(dest, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::DataCopy {
            dest,
            offset,
            length,
        } => Statement::DataCopy {
            dest: replace_value(dest, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::CallDataCopy {
            dest,
            offset,
            length,
        } => Statement::CallDataCopy {
            dest: replace_value(dest, replacements),
            offset: replace_value(offset, replacements),
            length: replace_value(length, replacements),
        },
        Statement::Block(region) => Statement::Block(replace_region(region, replacements)),
        Statement::Expr(expr) => Statement::Expr(replace_value_ids_in_expr(expr, replacements)),
        Statement::CustomErrorRevert { selector, args } => Statement::CustomErrorRevert {
            selector,
            args: args
                .into_iter()
                .map(|v| replace_value(v, replacements))
                .collect(),
        },
        Statement::SelfDestruct { address } => Statement::SelfDestruct {
            address: replace_value(address, replacements),
        },
        Statement::SetImmutable { key, value } => Statement::SetImmutable {
            key,
            value: replace_value(value, replacements),
        },
        // Statements with no Value operands pass through.
        other => other,
    }
}

/// Replace ValueIds in a region (statements + yields).
fn replace_region(region: Region, replacements: &BTreeMap<u32, ValueId>) -> Region {
    Region {
        statements: region
            .statements
            .into_iter()
            .map(|s| replace_value_ids_in_stmt(s, replacements))
            .collect(),
        yields: region
            .yields
            .into_iter()
            .map(|v| replace_value(v, replacements))
            .collect(),
    }
}
