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
///
/// Use counts are computed once over the WHOLE block (recursively, including every nested region)
/// and threaded into each per-list pass. A per-list count is unsound for fusion:
/// `Statement::Block` is scope-transparent — values defined inside it stay visible to the
/// enclosing scope (see `validate`) — so a keccak hash defined and `sload`ed inside a block but
/// also referenced by an ancestor statement looks single-use within the block's list. Fusing it
/// would delete the keccak definition while the outer reference still points at it, producing a
/// use-before-definition that the IR validator rejects (a compiler ICE on valid input). The
/// whole-block count sees the outer use and leaves such hashes alone.
fn outline_block(block: &mut Block, statistics: &mut CompoundOutliningStatistics) {
    let use_counts = count_value_uses(&block.statements);
    for statement in &mut block.statements {
        outline_nested_regions(statement, &use_counts, statistics);
    }
    outline_statements(&mut block.statements, &use_counts, statistics);
}

/// Recurse into nested regions within a statement.
fn outline_nested_regions(
    statement: &mut Statement,
    use_counts: &BTreeMap<u32, usize>,
    statistics: &mut CompoundOutliningStatistics,
) {
    match statement {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            outline_region(then_region, use_counts, statistics);
            if let Some(region) = else_region {
                outline_region(region, use_counts, statistics);
            }
        }
        Statement::Switch { cases, default, .. } => {
            for case in cases {
                outline_region(&mut case.body, use_counts, statistics);
            }
            if let Some(default_region) = default {
                outline_region(default_region, use_counts, statistics);
            }
        }
        Statement::For {
            condition_statements,
            body,
            post,
            ..
        } => {
            for statement in condition_statements.iter_mut() {
                outline_nested_regions(statement, use_counts, statistics);
            }
            outline_region(body, use_counts, statistics);
            outline_region(post, use_counts, statistics);
        }
        Statement::Block(region) => {
            outline_region(region, use_counts, statistics);
        }
        _ => {}
    }
}

/// Process a region.
fn outline_region(
    region: &mut Region,
    use_counts: &BTreeMap<u32, usize>,
    statistics: &mut CompoundOutliningStatistics,
) {
    for statement in &mut region.statements {
        outline_nested_regions(statement, use_counts, statistics);
    }
    outline_statements(&mut region.statements, use_counts, statistics);
}

/// Count how many times each ValueId is referenced (used) in a statement list,
/// recursing through nested regions and counting yields.
///
/// Call this on the whole enclosing block, not an inner list: fusion deletes a hash's
/// definition, which is only sound when the hash has no uses anywhere else, and scope-transparent
/// `Statement::Block`s let inner definitions be used by ancestor statements.
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
    use_counts: &BTreeMap<u32, usize>,
    statistics: &mut CompoundOutliningStatistics,
) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BitWidth, Type};
    use num::BigUint;

    fn value(id: u32) -> Value {
        Value {
            id: crate::ir::ValueId(id),
            value_type: Type::Int(BitWidth::I256),
        }
    }

    fn literal(id: u32, constant: u32) -> Statement {
        Statement::Let {
            bindings: vec![crate::ir::ValueId(id)],
            value: Expression::Literal {
                value: BigUint::from(constant),
                value_type: Type::Int(BitWidth::I256),
            },
        }
    }

    /// A keccak hash defined inside a scope-transparent `Statement::Block` and `sload`ed there,
    /// but *also* referenced by an ancestor statement, must NOT be fused: deleting its definition
    /// would dangle the outer reference. Regression for the per-list use-count bug.
    ///
    /// Value layout: word0 = v1, word1 = v2, stored value = v3, hash = v4, loaded value = v5.
    /// The outer `SStore` on v4 is an ancestor use of the block-defined hash that keeps it live.
    #[test]
    fn block_leaked_hash_not_fused() {
        let inner = Region {
            statements: vec![
                Statement::Let {
                    bindings: vec![crate::ir::ValueId(4)],
                    value: Expression::Keccak256Pair {
                        word0: value(1),
                        word1: value(2),
                    },
                },
                Statement::Let {
                    bindings: vec![crate::ir::ValueId(5)],
                    value: Expression::SLoad {
                        key: value(4),
                        static_slot: None,
                    },
                },
            ],
            yields: vec![],
        };
        let mut object = Object {
            name: "test".to_string(),
            code: Block {
                statements: vec![
                    literal(1, 0),
                    literal(2, 1),
                    literal(3, 42),
                    Statement::Block(inner),
                    Statement::SStore {
                        key: value(4),
                        value: value(3),
                        static_slot: None,
                    },
                ],
            },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let statistics = outline_compounds_in_object(&mut object);

        assert_eq!(
            statistics.mapping_sloads, 0,
            "hash used outside the block must not be fused"
        );
        assert!(
            crate::validate::validate_object(&object).is_ok(),
            "outlining must not leave a dangling reference: {:?}",
            crate::validate::validate_object(&object)
        );
    }

    /// Control case: a hash defined and used exactly once (no escaping use) is still fused, so the
    /// whole-block count does not regress legitimate outlining.
    #[test]
    fn single_use_hash_in_block_still_fused() {
        let inner = Region {
            statements: vec![
                Statement::Let {
                    bindings: vec![crate::ir::ValueId(4)],
                    value: Expression::Keccak256Pair {
                        word0: value(1),
                        word1: value(2),
                    },
                },
                Statement::Let {
                    bindings: vec![crate::ir::ValueId(5)],
                    value: Expression::SLoad {
                        key: value(4),
                        static_slot: None,
                    },
                },
            ],
            yields: vec![],
        };
        let mut object = Object {
            name: "test".to_string(),
            code: Block {
                statements: vec![literal(1, 0), literal(2, 1), Statement::Block(inner)],
            },
            functions: BTreeMap::new(),
            subobjects: vec![],
            data: BTreeMap::new(),
        };

        let statistics = outline_compounds_in_object(&mut object);

        assert_eq!(
            statistics.mapping_sloads, 1,
            "a single-use hash should still be fused into a mapping_sload"
        );
        assert!(crate::validate::validate_object(&object).is_ok());
    }
}
