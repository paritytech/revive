use petgraph::prelude::*;

use crate::cfg::Program;

pub mod control_flow;
pub mod dominance;
pub mod evm_bytecode;
pub mod types;

/// The analyzer visits each basic block using DFS.
pub trait BlockAnalysis: Default {
    fn analyze_block(&mut self, node: NodeIndex, program: &mut Program);

    fn apply_results(&mut self, program: &mut Program);
}

pub fn analyze<Pass>(program: &mut Program) -> Pass
where
    Pass: BlockAnalysis,
{
    let mut dfs = Dfs::new(&program.cfg.graph, program.cfg.start);
    let mut pass = Pass::default();

    while let Some(node) = dfs.next(&program.cfg.graph) {
        pass.analyze_block(node, program);
    }

    pass.apply_results(program);

    pass
}
