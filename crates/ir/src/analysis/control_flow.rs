use indexmap::{IndexMap, IndexSet};
use petgraph::prelude::*;

use crate::{
    analysis::BlockAnalysis,
    cfg::{Branch, Program},
    instruction::Instruction,
    symbol::Kind,
};

/// Remove basic blocks not reachable from the start node.
#[derive(Default)]
pub struct ReachableCode(pub IndexSet<NodeIndex>);

impl BlockAnalysis for ReachableCode {
    fn analyze_block(&mut self, node: NodeIndex, _program: &mut Program) {
        self.0.insert(node);
    }

    fn apply_results(&mut self, program: &mut Program) {
        program.cfg.graph.retain_nodes(|_, i| self.0.contains(&i));
    }
}

/// Remove edges to the jump table if the jump target is statically known.
#[derive(Default)]
pub struct StaticJumps(IndexMap<EdgeIndex, (NodeIndex, NodeIndex)>);

impl BlockAnalysis for StaticJumps {
    fn analyze_block(&mut self, node: NodeIndex, program: &mut Program) {
        for edge in program.cfg.graph.edges(node) {
            if *edge.weight() == Branch::Static {
                continue;
            }

            if let Some(Instruction::ConditionalBranch { target, .. })
            | Some(Instruction::UncoditionalBranch { target }) =
                program.cfg.graph[node].instructions.last()
            {
                let Kind::Constant(bytecode_offset) = target.symbol().kind else {
                    continue;
                };

                let destination = program
                    .jump_targets
                    .get(&bytecode_offset.as_usize())
                    .unwrap_or(&program.cfg.invalid_jump);

                self.0.insert(edge.id(), (node, *destination));
            }
        }
    }

    fn apply_results(&mut self, program: &mut Program) {
        for (edge, (a, b)) in &self.0 {
            program.cfg.graph.remove_edge(*edge);
            program.cfg.graph.add_edge(*a, *b, Branch::Static);
        }
    }
}
