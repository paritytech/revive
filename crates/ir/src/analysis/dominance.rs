use crate::{cfg::Program, instruction::Instruction, symbol::Type, POINTER_SIZE};
use petgraph::prelude::*;

use super::BlockAnalysis;

#[derive(Default)]
pub struct Unstack;

impl BlockAnalysis for Unstack {
    fn analyze_block(&mut self, node: NodeIndex, program: &mut Program) {
        for instruction in &program.cfg.graph[node].instructions {
            match instruction {
                _ => {}
            }
        }
    }

    fn apply_results(&mut self, _program: &mut Program) {}
}
