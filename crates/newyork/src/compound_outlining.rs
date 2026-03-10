//! Compound outlining pass for newyork IR.
//!
//! This pass detects multi-statement patterns in the IR and replaces them
//! with compound IR nodes that get lowered to single outlined function calls.
//! Runs after simplification and before LLVM codegen.
//!
//! Detected patterns:
//! - Mapping SLoad: `let hash = keccak256_pair(key, slot); let val = sload(hash)`
//!   → `let val = mapping_sload(key, slot)`
//! - Mapping SStore: `let hash = keccak256_pair(key, slot); sstore(hash, value)`
//!   → `mapping_sstore(key, slot, value)`
use std::collections::BTreeMap;

use crate::ir::{Block, Expr, Object, Region, Statement, Value};

/// Statistics from the compound outlining pass.
#[derive(Default, Debug)]
pub struct CompoundOutliningStats {
    /// Number of mapping sload patterns replaced.
    pub mapping_sloads: usize,
    /// Number of mapping sstore patterns replaced.
    pub mapping_sstores: usize,
}

/// Run compound outlining on an entire object tree (including subobjects).
pub fn outline_compounds_in_object(object: &mut Object) -> CompoundOutliningStats {
    let mut stats = CompoundOutliningStats::default();

    outline_block(&mut object.code, &mut stats);
    for func in object.functions.values_mut() {
        outline_block(&mut func.body, &mut stats);
    }

    for sub in &mut object.subobjects {
        let sub_stats = outline_compounds_in_object(sub);
        stats.mapping_sloads += sub_stats.mapping_sloads;
        stats.mapping_sstores += sub_stats.mapping_sstores;
    }

    stats
}

/// Process a block: detect and replace compound patterns.
fn outline_block(block: &mut Block, stats: &mut CompoundOutliningStats) {
    // First recurse into nested regions
    for stmt in &mut block.statements {
        outline_nested_regions(stmt, stats);
    }

    // Transform this level's statement list
    outline_statements(&mut block.statements, stats);
}

/// Recurse into nested regions within a statement.
fn outline_nested_regions(stmt: &mut Statement, stats: &mut CompoundOutliningStats) {
    match stmt {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            outline_region(then_region, stats);
            if let Some(r) = else_region {
                outline_region(r, stats);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                outline_region(&mut case.body, stats);
            }
            if let Some(d) = default {
                outline_region(d, stats);
            }
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            for s in condition_stmts.iter_mut() {
                outline_nested_regions(s, stats);
            }
            outline_region(body, stats);
            outline_region(post, stats);
        }
        Statement::Block(region) => {
            outline_region(region, stats);
        }
        _ => {}
    }
}

/// Process a region.
fn outline_region(region: &mut Region, stats: &mut CompoundOutliningStats) {
    for stmt in &mut region.statements {
        outline_nested_regions(stmt, stats);
    }
    outline_statements(&mut region.statements, stats);
}

/// Count how many times each ValueId is referenced (used) in a flat statement list.
/// This only counts at the current scope level (including nested regions).
fn count_value_uses(stmts: &[Statement]) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for stmt in stmts {
        count_uses_in_stmt(stmt, &mut counts);
    }
    counts
}

fn inc_use(counts: &mut BTreeMap<u32, usize>, val: &Value) {
    *counts.entry(val.id.0).or_insert(0) += 1;
}

fn count_uses_in_expr(expr: &Expr, counts: &mut BTreeMap<u32, usize>) {
    match expr {
        Expr::Var(id) => {
            *counts.entry(id.0).or_insert(0) += 1;
        }
        Expr::Binary { lhs, rhs, .. } => {
            inc_use(counts, lhs);
            inc_use(counts, rhs);
        }
        Expr::Ternary { a, b, n, .. } => {
            inc_use(counts, a);
            inc_use(counts, b);
            inc_use(counts, n);
        }
        Expr::Unary { operand, .. }
        | Expr::CallDataLoad { offset: operand }
        | Expr::ExtCodeSize { address: operand }
        | Expr::ExtCodeHash { address: operand }
        | Expr::BlockHash { number: operand }
        | Expr::BlobHash { index: operand }
        | Expr::Balance { address: operand }
        | Expr::MLoad {
            offset: operand, ..
        }
        | Expr::Keccak256Single { word0: operand } => {
            inc_use(counts, operand);
        }
        Expr::SLoad { key, .. } | Expr::TLoad { key } => inc_use(counts, key),
        Expr::Keccak256 { offset, length } => {
            inc_use(counts, offset);
            inc_use(counts, length);
        }
        Expr::Keccak256Pair { word0, word1 }
        | Expr::MappingSLoad {
            key: word0,
            slot: word1,
        } => {
            inc_use(counts, word0);
            inc_use(counts, word1);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                inc_use(counts, arg);
            }
        }
        Expr::Truncate { value, .. }
        | Expr::ZeroExtend { value, .. }
        | Expr::SignExtendTo { value, .. } => inc_use(counts, value),
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
    }
}

fn count_uses_in_region(region: &Region, counts: &mut BTreeMap<u32, usize>) {
    for stmt in &region.statements {
        count_uses_in_stmt(stmt, counts);
    }
    for y in &region.yields {
        inc_use(counts, y);
    }
}

fn count_uses_in_stmt(stmt: &Statement, counts: &mut BTreeMap<u32, usize>) {
    match stmt {
        Statement::Let { value, .. } | Statement::Expr(value) => {
            count_uses_in_expr(value, counts);
        }
        Statement::MStore { offset, value, .. } | Statement::MStore8 { offset, value, .. } => {
            inc_use(counts, offset);
            inc_use(counts, value);
        }
        Statement::MCopy { dest, src, length } => {
            inc_use(counts, dest);
            inc_use(counts, src);
            inc_use(counts, length);
        }
        Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
            inc_use(counts, key);
            inc_use(counts, value);
        }
        Statement::MappingSStore { key, slot, value } => {
            inc_use(counts, key);
            inc_use(counts, slot);
            inc_use(counts, value);
        }
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            ..
        } => {
            inc_use(counts, condition);
            for i in inputs {
                inc_use(counts, i);
            }
            count_uses_in_region(then_region, counts);
            if let Some(r) = else_region {
                count_uses_in_region(r, counts);
            }
        }
        Statement::Switch {
            scrutinee,
            inputs,
            cases,
            default,
            ..
        } => {
            inc_use(counts, scrutinee);
            for i in inputs {
                inc_use(counts, i);
            }
            for c in cases {
                count_uses_in_region(&c.body, counts);
            }
            if let Some(d) = default {
                count_uses_in_region(d, counts);
            }
        }
        Statement::For {
            init_values,
            condition_stmts,
            condition,
            body,
            post,
            ..
        } => {
            for v in init_values {
                inc_use(counts, v);
            }
            for s in condition_stmts {
                count_uses_in_stmt(s, counts);
            }
            count_uses_in_expr(condition, counts);
            count_uses_in_region(body, counts);
            count_uses_in_region(post, counts);
        }
        Statement::Revert { offset, length } | Statement::Return { offset, length } => {
            inc_use(counts, offset);
            inc_use(counts, length);
        }
        Statement::ExternalCall {
            gas,
            address,
            value,
            args_offset,
            args_length,
            ret_offset,
            ret_length,
            ..
        } => {
            inc_use(counts, gas);
            inc_use(counts, address);
            if let Some(v) = value {
                inc_use(counts, v);
            }
            inc_use(counts, args_offset);
            inc_use(counts, args_length);
            inc_use(counts, ret_offset);
            inc_use(counts, ret_length);
        }
        Statement::Create {
            value,
            offset,
            length,
            salt,
            ..
        } => {
            inc_use(counts, value);
            inc_use(counts, offset);
            inc_use(counts, length);
            if let Some(s) = salt {
                inc_use(counts, s);
            }
        }
        Statement::Log {
            offset,
            length,
            topics,
        } => {
            inc_use(counts, offset);
            inc_use(counts, length);
            for t in topics {
                inc_use(counts, t);
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
            inc_use(counts, dest);
            inc_use(counts, offset);
            inc_use(counts, length);
        }
        Statement::ExtCodeCopy {
            address,
            dest,
            offset,
            length,
        } => {
            inc_use(counts, address);
            inc_use(counts, dest);
            inc_use(counts, offset);
            inc_use(counts, length);
        }
        Statement::SetImmutable { value, .. } => inc_use(counts, value),
        Statement::Leave { return_values } => {
            for v in return_values {
                inc_use(counts, v);
            }
        }
        Statement::SelfDestruct { address } => inc_use(counts, address),
        Statement::Block(region) => count_uses_in_region(region, counts),
        Statement::Break { values } | Statement::Continue { values } => {
            for v in values {
                inc_use(counts, v);
            }
        }
        Statement::CustomErrorRevert { args, .. } => {
            for arg in args {
                inc_use(counts, arg);
            }
        }
        Statement::Stop
        | Statement::Invalid
        | Statement::PanicRevert { .. }
        | Statement::ErrorStringRevert { .. } => {}
    }
}

/// Core transformation: scan a statement list for compound patterns and replace them.
///
/// Detects two patterns:
/// 1. MappingSLoad: `let hash = keccak256_pair(key, slot); ... let val = sload(hash)`
///    where hash has exactly one use (the sload).
/// 2. MappingSStore: `let hash = keccak256_pair(key, slot); ... sstore(hash, value)`
///    where hash has exactly one use (the sstore).
fn outline_statements(stmts: &mut Vec<Statement>, stats: &mut CompoundOutliningStats) {
    // Build use counts for the entire statement list
    let use_counts = count_value_uses(stmts);

    // Build a map from ValueId → (index, keccak word0, keccak word1)
    // for all single-binding Let statements whose value is Keccak256Pair.
    let mut keccak_defs: BTreeMap<u32, (usize, Value, Value)> = BTreeMap::new();
    for (idx, stmt) in stmts.iter().enumerate() {
        if let Statement::Let {
            bindings,
            value: Expr::Keccak256Pair { word0, word1 },
        } = stmt
        {
            if bindings.len() == 1 {
                keccak_defs.insert(bindings[0].0, (idx, *word0, *word1));
            }
        }
    }

    if keccak_defs.is_empty() {
        return;
    }

    // Collect transformations: (stmt_index, new_statement, keccak_def_index_to_remove)
    let mut transforms: Vec<(usize, Statement, usize)> = Vec::new();

    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            // Pattern 1: let val = sload(hash) where hash = keccak256_pair(key, slot)
            Statement::Let {
                bindings,
                value: Expr::SLoad { key, .. },
            } if bindings.len() == 1 => {
                if let Some((def_idx, word0, word1)) = keccak_defs.get(&key.id.0) {
                    if use_counts.get(&key.id.0).copied().unwrap_or(0) == 1 {
                        transforms.push((
                            idx,
                            Statement::Let {
                                bindings: bindings.clone(),
                                value: Expr::MappingSLoad {
                                    key: *word0,
                                    slot: *word1,
                                },
                            },
                            *def_idx,
                        ));
                    }
                }
            }
            // Pattern 2: sstore(hash, value) where hash = keccak256_pair(key, slot)
            Statement::SStore { key, value, .. } => {
                if let Some((def_idx, word0, word1)) = keccak_defs.get(&key.id.0) {
                    if use_counts.get(&key.id.0).copied().unwrap_or(0) == 1 {
                        transforms.push((
                            idx,
                            Statement::MappingSStore {
                                key: *word0,
                                slot: *word1,
                                value: *value,
                            },
                            *def_idx,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    if transforms.is_empty() {
        return;
    }

    // Collect all indices to remove (keccak definitions that are absorbed)
    let mut indices_to_remove = std::collections::BTreeSet::new();
    let mut replacements: BTreeMap<usize, Statement> = BTreeMap::new();

    for (stmt_idx, new_stmt, def_idx) in transforms {
        // Only apply if neither index was already claimed
        if !indices_to_remove.contains(&def_idx) && !indices_to_remove.contains(&stmt_idx) {
            indices_to_remove.insert(def_idx);
            replacements.insert(stmt_idx, new_stmt);
            match &replacements[&stmt_idx] {
                Statement::Let {
                    value: Expr::MappingSLoad { .. },
                    ..
                } => stats.mapping_sloads += 1,
                Statement::MappingSStore { .. } => stats.mapping_sstores += 1,
                _ => {}
            }
        }
    }

    // Apply replacements
    for (idx, new_stmt) in &replacements {
        stmts[*idx] = new_stmt.clone();
    }

    // Remove absorbed keccak definitions (iterate in reverse to preserve indices)
    for idx in indices_to_remove.into_iter().rev() {
        stmts.remove(idx);
    }
}
