//! Compound outlining pass for newyork IR.
//!
//! This pass detects multi-statement patterns in the IR and replaces them
//! with compound IR nodes that get lowered to single outlined function calls.
//! Runs after simplification and before LLVM codegen.
//!
//! Detected patterns:
//! - Mapping SLoad: `let hash = keccak256_pair(key, slot); let value = sload(hash)`
//!   → `let value = mapping_sload(key, slot)`
//! - Mapping SStore: `let hash = keccak256_pair(key, slot); sstore(hash, value)`
//!   → `mapping_sstore(key, slot, value)`
use std::collections::BTreeMap;

use crate::ir::{Block, Expression, Object, Region, Statement, Value};

/// Statistics from the compound outlining pass.
#[derive(Default, Debug)]
pub struct CompoundOutliningStatistics {
    /// Number of mapping sload patterns replaced.
    pub mapping_sloads: usize,
    /// Number of mapping sstore patterns replaced.
    pub mapping_sstores: usize,
}

/// Run compound outlining on an entire object tree (including subobjects).
pub fn outline_compounds_in_object(object: &mut Object) -> CompoundOutliningStatistics {
    let mut statistics = CompoundOutliningStatistics::default();

    outline_block(&mut object.code, &mut statistics);
    for function in object.functions.values_mut() {
        outline_block(&mut function.body, &mut statistics);
    }

    for sub_object in &mut object.subobjects {
        let sub_object_statistics = outline_compounds_in_object(sub_object);
        statistics.mapping_sloads += sub_object_statistics.mapping_sloads;
        statistics.mapping_sstores += sub_object_statistics.mapping_sstores;
    }

    statistics
}

/// Process a block: detect and replace compound patterns.
fn outline_block(block: &mut Block, statistics: &mut CompoundOutliningStatistics) {
    for statement in &mut block.statements {
        outline_nested_regions(statement, statistics);
    }
    outline_statements(&mut block.statements, statistics);
}

/// Recurse into nested regions within a statement.
fn outline_nested_regions(statement: &mut Statement, statistics: &mut CompoundOutliningStatistics) {
    match statement {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            outline_region(then_region, statistics);
            if let Some(region) = else_region {
                outline_region(region, statistics);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                outline_region(&mut case.body, statistics);
            }
            if let Some(default_region) = default {
                outline_region(default_region, statistics);
            }
        }
        Statement::For {
            condition_statements,
            body,
            post,
            ..
        } => {
            for statement in condition_statements.iter_mut() {
                outline_nested_regions(statement, statistics);
            }
            outline_region(body, statistics);
            outline_region(post, statistics);
        }
        Statement::Block(region) => {
            outline_region(region, statistics);
        }
        _ => {}
    }
}

/// Process a region.
fn outline_region(region: &mut Region, statistics: &mut CompoundOutliningStatistics) {
    for statement in &mut region.statements {
        outline_nested_regions(statement, statistics);
    }
    outline_statements(&mut region.statements, statistics);
}

/// Count how many times each ValueId is referenced (used) in a statement list,
/// recursing through nested regions and counting yields.
fn count_value_uses(statements: &[Statement]) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for statement in statements {
        statement.for_each_value_id(&mut |id| {
            *counts.entry(id.0).or_insert(0) += 1;
        });
    }
    counts
}

/// Core transformation: scan a statement list for compound patterns and replace them.
///
/// Detects two patterns:
/// 1. MappingSLoad: `let hash = keccak256_pair(key, slot); ... let value = sload(hash)`
///    where hash has exactly one use (the sload).
/// 2. MappingSStore: `let hash = keccak256_pair(key, slot); ... sstore(hash, value)`
///    where hash has exactly one use (the sstore).
fn outline_statements(
    statements: &mut Vec<Statement>,
    statistics: &mut CompoundOutliningStatistics,
) {
    let use_counts = count_value_uses(statements);

    let mut keccak_definitions: BTreeMap<u32, (usize, Value, Value)> = BTreeMap::new();
    for (index, statement) in statements.iter().enumerate() {
        if let Statement::Let {
            bindings,
            value: Expression::Keccak256Pair { word0, word1 },
        } = statement
        {
            if bindings.len() == 1 {
                keccak_definitions.insert(bindings[0].0, (index, *word0, *word1));
            }
        }
    }

    if keccak_definitions.is_empty() {
        return;
    }

    let mut transformations: Vec<(usize, Statement, usize)> = Vec::new();

    for (index, statement) in statements.iter().enumerate() {
        match statement {
            Statement::Let {
                bindings,
                value: Expression::SLoad { key, .. },
            } if bindings.len() == 1 => {
                if let Some((definition_index, word0, word1)) = keccak_definitions.get(&key.id.0) {
                    if use_counts.get(&key.id.0).copied().unwrap_or(0) == 1 {
                        transformations.push((
                            index,
                            Statement::Let {
                                bindings: bindings.clone(),
                                value: Expression::MappingSLoad {
                                    key: *word0,
                                    slot: *word1,
                                },
                            },
                            *definition_index,
                        ));
                    }
                }
            }
            Statement::SStore { key, value, .. } => {
                if let Some((definition_index, word0, word1)) = keccak_definitions.get(&key.id.0) {
                    if use_counts.get(&key.id.0).copied().unwrap_or(0) == 1 {
                        transformations.push((
                            index,
                            Statement::MappingSStore {
                                key: *word0,
                                slot: *word1,
                                value: *value,
                            },
                            *definition_index,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    if transformations.is_empty() {
        return;
    }

    let mut indices_to_remove = std::collections::BTreeSet::new();
    let mut replacements: BTreeMap<usize, Statement> = BTreeMap::new();

    for (statement_index, new_statement, definition_index) in transformations {
        if !indices_to_remove.contains(&definition_index)
            && !indices_to_remove.contains(&statement_index)
        {
            indices_to_remove.insert(definition_index);
            replacements.insert(statement_index, new_statement);
            match &replacements[&statement_index] {
                Statement::Let {
                    value: Expression::MappingSLoad { .. },
                    ..
                } => statistics.mapping_sloads += 1,
                Statement::MappingSStore { .. } => statistics.mapping_sstores += 1,
                _ => {}
            }
        }
    }

    for (index, new_statement) in &replacements {
        statements[*index] = new_statement.clone();
    }

    for index in indices_to_remove.into_iter().rev() {
        statements.remove(index);
    }
}
