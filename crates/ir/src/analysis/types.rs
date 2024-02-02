use crate::{cfg::Program, instruction::Instruction, symbol::Type, POINTER_SIZE};
use petgraph::prelude::*;

use super::BlockAnalysis;

#[derive(Default)]
pub struct TypePropagation;

impl BlockAnalysis for TypePropagation {
    fn analyze_block(&mut self, node: NodeIndex, program: &mut Program) {
        for instruction in &program.cfg.graph[node].instructions {
            match instruction {
                Instruction::ConditionalBranch { condition, target } => {
                    condition.replace_type(Type::Bool);
                    target.replace_type(Type::Int(POINTER_SIZE));
                }

                Instruction::UncoditionalBranch { target } => {
                    target.replace_type(Type::Int(POINTER_SIZE));
                }

                Instruction::BinaryAssign { x, y, z, .. } => {
                    y.replace_type(x.symbol().type_hint);
                    z.replace_type(x.symbol().type_hint);
                }

                Instruction::Copy { x, y } | Instruction::UnaryAssign { x, y, .. } => {
                    x.replace_type(y.symbol().type_hint);
                }

                Instruction::IndexedCopy { index, .. }
                | Instruction::IndexedAssign { index, .. } => {
                    index.replace_type(Type::Int(POINTER_SIZE))
                }

                _ => {}
            }
        }
    }

    fn apply_results(&mut self, _program: &mut Program) {}
}
