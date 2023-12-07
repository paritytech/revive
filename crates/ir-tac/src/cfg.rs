use std::fmt::Write;
use std::ops::Range;

use evmil::bytecode;
use petgraph::{
    dot::{Config, Dot},
    graph::DiGraph,
    stable_graph::NodeIndex,
};

use crate::{
    instruction::{self, Instruction},
    symbol::SymbolTable,
};

#[derive(Clone, Debug)]
pub struct EvmInstruction {
    pub bytecode_offset: usize,
    pub instruction: bytecode::Instruction,
}

#[derive(Debug, Default)]
pub struct BasicBlock {
    pub entry: Option<Entry>,
    pub opcodes: Range<usize>,
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Copy, Default)]
pub enum BasicBlockFormatOption {
    ByteCode,
    Ir,
    #[default]
    None,
}

impl BasicBlock {
    fn format(&self, evm_bytecode: &[EvmInstruction], options: BasicBlockFormatOption) -> String {
        let offset = evm_bytecode[self.opcodes.start].bytecode_offset;
        let start = if let Some(Entry::Start) = self.entry {
            "Start\n".to_string()
        } else {
            String::new()
        };
        let instructions = match options {
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
        };

        format!("{start}Offset: 0x{offset:02x}\n---\n{instructions}")
    }
}

#[derive(Clone, Debug)]
pub enum Entry {
    Start,
    Jumpdest(NodeIndex),
    Else(NodeIndex),
}

#[derive(Debug)]
pub enum Jump {
    Direct,
    Indirect,
}

pub struct Program {
    pub evm_instructions: Vec<EvmInstruction>,
    pub cfg: DiGraph<BasicBlock, Jump>,
    pub symbol_table: SymbolTable,
}

impl Program {
    pub fn new(bytecode: Vec<bytecode::Instruction>) -> Self {
        let mut cfg = DiGraph::new();
        let mut symbol_table = SymbolTable::default();
        let mut evm_instructions = Vec::with_capacity(bytecode.len());

        let mut current_block = Some(BasicBlock {
            entry: Some(Entry::Start),
            ..Default::default()
        });
        let mut bytecode_offset = 0;

        for (index, opcode) in bytecode.iter().enumerate() {
            evm_instructions.push(EvmInstruction {
                bytecode_offset,
                instruction: opcode.clone(),
            });
            bytecode_offset += opcode.length();

            let instructions = instruction::translate(opcode, &mut symbol_table);

            use bytecode::Instruction::*;
            match opcode {
                JUMPDEST => {
                    // If we are already in a bb, conclude it
                    let entry = current_block.take().map(|mut node| {
                        node.opcodes.end = index + 1;
                        let entry = node.entry.clone();
                        let node_index = cfg.add_node(node);

                        // If the block had an entry, add an edge from the previous block to it
                        if let Some(Entry::Else(incoming)) | Some(Entry::Jumpdest(incoming)) = entry
                        {
                            cfg.add_edge(incoming, node_index, Jump::Direct);
                        }
                        node_index
                    });

                    // JUMPDEST implicitly starts a new bb
                    current_block = Some(BasicBlock {
                        entry: entry.map(Entry::Jumpdest),
                        opcodes: Range {
                            start: index + 1,
                            end: index + 1,
                        },
                        ..Default::default()
                    });
                }

                JUMP | STOP | RETURN | REVERT | INVALID => {
                    // Conclude this bb; if we are not already in a bb we must create a new one
                    let mut node = current_block.take().unwrap_or_else(|| BasicBlock {
                        opcodes: Range {
                            start: index,
                            end: index + 1,
                        },
                        ..Default::default()
                    });
                    node.instructions.extend(instructions);
                    node.opcodes.end = index + 1;

                    let entry = node.entry.clone();
                    let node_index = cfg.add_node(node);

                    // If the block had an entry, add an edge from the previous block to it
                    if let Some(Entry::Else(incoming)) | Some(Entry::Jumpdest(incoming)) = entry {
                        cfg.add_edge(incoming, node_index, Jump::Direct);
                    }
                }

                JUMPI => {
                    // Conclude this bb; if we are not already in a bb we must create a new one
                    let mut node = current_block.take().unwrap_or_else(|| BasicBlock {
                        opcodes: Range {
                            start: index,
                            end: index + 1,
                        },
                        ..Default::default()
                    });
                    node.instructions.extend(instructions);
                    node.opcodes.end = index + 1;

                    let entry = node.entry.clone();
                    let node_index = cfg.add_node(node);

                    // If the block had an entry, add an edge from the previous block to it
                    if let Some(Entry::Else(incoming)) | Some(Entry::Jumpdest(incoming)) = entry {
                        cfg.add_edge(incoming, node_index, Jump::Direct);
                    }

                    // JUMPI implicitly starts a new bb for the else branch
                    current_block = Some(BasicBlock {
                        entry: Some(Entry::Else(node_index)),
                        opcodes: Range {
                            start: index + 1,
                            end: index + 1,
                        },
                        ..Default::default()
                    });
                }

                _ => current_block
                    .get_or_insert(BasicBlock {
                        opcodes: Range {
                            start: index,
                            end: index + 1,
                        },
                        ..Default::default()
                    })
                    .instructions
                    .extend(instructions),
            }
        }

        Self {
            evm_instructions,
            cfg,
            symbol_table,
        }
    }

    pub fn dot(&self, format_options: BasicBlockFormatOption) {
        let get_node_attrs = move |_, (_, node): (_, &BasicBlock)| {
            format!(
                "label = \"{}\"",
                node.format(&self.evm_instructions, format_options)
            )
        };

        let dot = Dot::with_attr_getters(
            &self.cfg,
            &[Config::EdgeNoLabel, Config::NodeNoLabel],
            &|_, edge| format!("label = \"{:?}\"", edge.weight()),
            &get_node_attrs,
        );

        println!("{dot:?}");
    }
}
