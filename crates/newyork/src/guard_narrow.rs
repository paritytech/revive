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
    let mut next_id = find_max_value_id(object) + 1;
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

/// Returns true if `val` is a useful boundary mask for guard narrowing.
/// Only masks that fit in a native register width (≤64 bits) are useful,
/// since the codegen can narrow arithmetic and comparisons to i64.
/// Larger masks (e.g., 128-bit, 160-bit, 192-bit) don't benefit from
/// narrowing because there's no efficient native type for them.
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

/// Find the maximum ValueId in use across the entire object tree.
fn find_max_value_id(object: &Object) -> u32 {
    let mut max_id = 0u32;

    fn visit_value(val: &Value, max_id: &mut u32) {
        *max_id = (*max_id).max(val.id.0);
    }

    fn visit_expr(expr: &Expr, max_id: &mut u32) {
        match expr {
            Expr::Literal { .. }
            | Expr::CallValue
            | Expr::Caller
            | Expr::Origin
            | Expr::CallDataSize
            | Expr::CodeSize
            | Expr::GasPrice
            | Expr::ReturnDataSize
            | Expr::Coinbase
            | Expr::Timestamp
            | Expr::Number
            | Expr::Difficulty
            | Expr::GasLimit
            | Expr::ChainId
            | Expr::SelfBalance
            | Expr::BaseFee
            | Expr::BlobBaseFee
            | Expr::Gas
            | Expr::MSize
            | Expr::Address
            | Expr::DataOffset { .. }
            | Expr::DataSize { .. }
            | Expr::LoadImmutable { .. }
            | Expr::LinkerSymbol { .. } => {}
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
            Expr::ExtCodeSize { address }
            | Expr::ExtCodeHash { address }
            | Expr::Balance { address } => visit_value(address, max_id),
            Expr::BlockHash { number } => visit_value(number, max_id),
            Expr::BlobHash { index } => visit_value(index, max_id),
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
            Expr::Keccak256Pair { word0, word1 } => {
                visit_value(word0, max_id);
                visit_value(word1, max_id);
            }
            Expr::Keccak256Single { word0 } => {
                visit_value(word0, max_id);
            }
        }
    }

    fn visit_region(region: &Region, max_id: &mut u32) {
        for stmt in &region.statements {
            visit_stmt(stmt, max_id);
        }
        for y in &region.yields {
            visit_value(y, max_id);
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
            Statement::MStore { offset, value, .. } | Statement::MStore8 { offset, value, .. } => {
                visit_value(offset, max_id);
                visit_value(value, max_id);
            }
            Statement::MCopy { dest, src, length } => {
                visit_value(dest, max_id);
                visit_value(src, max_id);
                visit_value(length, max_id);
            }
            Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
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
                for v in inputs {
                    visit_value(v, max_id);
                }
                visit_region(then_region, max_id);
                if let Some(r) = else_region {
                    visit_region(r, max_id);
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
                for v in inputs {
                    visit_value(v, max_id);
                }
                for c in cases {
                    visit_region(&c.body, max_id);
                }
                if let Some(d) = default {
                    visit_region(d, max_id);
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
                post_input_vars,
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
                visit_region(body, max_id);
                for pv in post_input_vars {
                    *max_id = (*max_id).max(pv.0);
                }
                visit_region(post, max_id);
                for o in outputs {
                    *max_id = (*max_id).max(o.0);
                }
            }
            Statement::Block(region) => visit_region(region, max_id),
            Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                visit_value(offset, max_id);
                visit_value(length, max_id);
            }
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
            Statement::CustomErrorRevert { args, .. } => {
                for a in args {
                    visit_value(a, max_id);
                }
            }
            Statement::SelfDestruct { address } => visit_value(address, max_id),
            Statement::SetImmutable { value, .. } => visit_value(value, max_id),
            Statement::Expr(expr) => visit_expr(expr, max_id),
            Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::Leave { .. } => {
                // Break/Continue/Leave have values but they're handled above
                // Actually check...
            }
        }
        // Handle Break/Continue/Leave values
        match stmt {
            Statement::Break { values }
            | Statement::Continue { values }
            | Statement::Leave {
                return_values: values,
            } => {
                for v in values {
                    visit_value(v, max_id);
                }
            }
            _ => {}
        }
    }

    // Visit main code block.
    for stmt in &object.code.statements {
        visit_stmt(stmt, &mut max_id);
    }

    // Visit all functions.
    for func in object.functions.values() {
        for param in &func.params {
            max_id = max_id.max(param.0 .0);
        }
        for ret in &func.return_values {
            max_id = max_id.max(ret.0);
        }
        for stmt in &func.body.statements {
            visit_stmt(stmt, &mut max_id);
        }
    }

    // Visit subobjects.
    for sub in &object.subobjects {
        max_id = max_id.max(find_max_value_id(sub));
    }

    max_id
}
