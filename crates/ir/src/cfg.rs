use std::fmt::Write;
use std::ops::Range;

use indexmap::IndexMap;
use petgraph::dot::{Config, Dot};
use petgraph::prelude::*;

use crate::pass::dead_code::DeadCodeElimination;
use crate::pass::lift::BytecodeLifter;
use crate::pass::Pass;
use crate::symbol::SymbolRef;
use crate::{instruction::Instruction, symbol::SymbolTable};

pub struct Cfg {
    pub graph: StableDiGraph<BasicBlock, Branch>,
    pub start: NodeIndex,
    pub jump_table: NodeIndex,
    pub terminator: NodeIndex,
    pub invalid_jump: NodeIndex,
}

#[derive(Debug, PartialEq)]
pub enum Branch {
    Static,
    Dynamic,
}

impl Default for Cfg {
    fn default() -> Self {
        let mut graph = StableDiGraph::new();

        Self {
            start: graph.add_node(Default::default()),
            jump_table: graph.add_node(Default::default()),
            terminator: graph.add_node(Default::default()),
            invalid_jump: graph.add_node(Default::default()),
            graph,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvmInstruction {
    pub bytecode_offset: usize,
    pub instruction: evmil::bytecode::Instruction,
}

#[derive(Debug, Default)]
pub struct BasicBlock {
    pub opcodes: Range<usize>,
    pub instructions: Vec<Instruction>,
    pub stack_info: StackInfo,
}

#[derive(Debug, Default)]
pub struct StackInfo {
    pub arguments: usize,
    pub generates: Vec<SymbolRef>,
    pub height: i32,
}

impl BasicBlock {
    fn linear_at(start: usize) -> Self {
        Self {
            opcodes: start..start + 1,
            ..Default::default()
        }
    }

    fn format(&self, evm_bytecode: &[EvmInstruction], options: BasicBlockFormatOption) -> String {
        match options {
            BasicBlockFormatOption::ByteCode => evm_bytecode[self.opcodes.start..self.opcodes.end]
                .iter()
                .fold(String::new(), |mut acc, opcode| {
                    writeln!(&mut acc, "{:?}", opcode.instruction).unwrap();
                    acc
                }),
            BasicBlockFormatOption::Ir => {
                self.instructions
                    .iter()
                    .fold(String::new(), |mut acc, instruction| {
                        writeln!(&mut acc, "{instruction}").unwrap();
                        acc
                    })
            }
            _ => String::new(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum BasicBlockFormatOption {
    ByteCode,
    Ir,
    #[default]
    None,
}

pub struct Program {
    pub evm_instructions: Vec<EvmInstruction>,
    pub cfg: Cfg,
    pub symbol_table: SymbolTable,
    pub jump_targets: IndexMap<usize, NodeIndex>,
}

impl Program {
    /// Create a new [Program] from EVM bytecode.
    ///
    /// - Dynamic jumps reach the dynamic jump table
    /// - `JUMPDEST` and `JUMPI` split up the node
    /// - Instructions not returning reach the terminator node
    pub fn new(bytecode: &[evmil::bytecode::Instruction]) -> Self {
        let mut evm_instructions = Vec::with_capacity(bytecode.len());
        let mut cfg = Cfg::default();
        let mut jump_targets = IndexMap::default();
        let mut bytecode_offset = 0;
        let mut node = cfg.graph.add_node(Default::default());
        cfg.graph.add_edge(cfg.start, node, Branch::Static);
        cfg.graph
            .add_edge(cfg.invalid_jump, cfg.terminator, Branch::Static);
        cfg.graph
            .add_edge(cfg.jump_table, cfg.invalid_jump, Branch::Dynamic);

        for (index, opcode) in bytecode.iter().enumerate() {
            evm_instructions.push(EvmInstruction {
                bytecode_offset,
                instruction: opcode.clone(),
            });
            cfg.graph[node].opcodes.end = index + 1;

            use evmil::bytecode::Instruction::*;
            match opcode {
                // The preceding instruction did already split up control flow
                JUMPDEST
                    if matches!(
                        evm_instructions[index.saturating_sub(1)].instruction,
                        JUMP | JUMPI | RETURN | REVERT | INVALID | STOP | SELFDESTRUCT
                    ) =>
                {
                    cfg.graph.add_edge(cfg.jump_table, node, Branch::Dynamic);

                    jump_targets.insert(bytecode_offset, node);
                }

                JUMPDEST => {
                    cfg.graph[node].opcodes.end = index;
                    let previous_node = node;
                    node = cfg.graph.add_node(BasicBlock::linear_at(index));

                    cfg.graph.add_edge(cfg.jump_table, node, Branch::Dynamic);
                    cfg.graph.add_edge(previous_node, node, Branch::Static);

                    jump_targets.insert(bytecode_offset, node);
                }

                JUMP => {
                    cfg.graph.add_edge(node, cfg.jump_table, Branch::Dynamic);

                    node = cfg.graph.add_node(BasicBlock::linear_at(index + 1));
                }

                JUMPI => {
                    cfg.graph.add_edge(node, cfg.jump_table, Branch::Dynamic);

                    let previous_node = node;
                    node = cfg.graph.add_node(BasicBlock::linear_at(index + 1));
                    cfg.graph.add_edge(previous_node, node, Branch::Static);
                }

                STOP | RETURN | REVERT | INVALID | SELFDESTRUCT => {
                    cfg.graph.add_edge(node, cfg.terminator, Branch::Static);

                    node = cfg.graph.add_node(BasicBlock::linear_at(index + 1));
                }

                _ => {}
            }

            bytecode_offset += opcode.length();
        }

        Self {
            evm_instructions,
            cfg,
            symbol_table: Default::default(),
            jump_targets,
        }
    }

    pub fn optimize(&mut self) {
        DeadCodeElimination::run(&mut Default::default(), self);
        BytecodeLifter::run(&mut Default::default(), self);
        DeadCodeElimination::run(&mut Default::default(), self)
    }

    pub fn dot(&self, format_options: BasicBlockFormatOption) {
        let get_node_attrs = move |_, (index, node): (_, &BasicBlock)| {
            let (color, shape, label) = if index == self.cfg.terminator {
                ("red", "oval", "Terminator".to_string())
            } else if index == self.cfg.start {
                ("red", "oval", "Start".to_string())
            } else if index == self.cfg.invalid_jump {
                ("blue", "hexagon", "Invalid jump target".to_string())
            } else if index == self.cfg.jump_table {
                ("blue", "diamond", "Dynamic jump table".to_string())
            } else {
                let instructions = node.format(&self.evm_instructions, format_options);
                let start = &self.evm_instructions[node.opcodes.start].bytecode_offset;
                let end = &self
                    .evm_instructions
                    .get(node.opcodes.end)
                    .unwrap_or_else(|| &self.evm_instructions[node.opcodes.end - 1])
                    .bytecode_offset;
                (
                    "black",
                    "rectangle",
                    format!("Bytecode (0x{start:02x}, 0x{end:02x}]\n---\n{instructions}",),
                )
            };

            format!("color={color} shape={shape} label=\"{label}\"",)
        };

        let get_edge_attrs = |_, edge: petgraph::stable_graph::EdgeReference<'_, Branch>| {
            let style = match edge.weight() {
                Branch::Static => "solid",
                Branch::Dynamic => "dashed",
            };
            format!("style={style}")
        };

        let dot = Dot::with_attr_getters(
            &self.cfg.graph,
            &[Config::EdgeNoLabel, Config::NodeNoLabel],
            &get_edge_attrs,
            &get_node_attrs,
        );

        println!("{dot:?}");
    }
}
