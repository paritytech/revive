use indexmap::{IndexMap, IndexSet};
use petgraph::prelude::*;

use crate::{
    analysis::BlockAnalysis,
    cfg::{Program, StackInfo},
    instruction::{Instruction, Operator},
    symbol::{Global, Kind, Symbol, SymbolBuilder, SymbolRef, SymbolTable},
};

#[derive(Default)]
pub struct IrBuilder;

impl BlockAnalysis for IrBuilder {
    fn analyze_block(&mut self, node: NodeIndex, program: &mut Program) {
        let mut builder = BlockBuilder::new(node, &mut program.symbol_table);

        for opcode in &program.evm_instructions[program.cfg.graph[node].opcodes.to_owned()] {
            builder.translate(&opcode.instruction);
        }

        let (instructions, stack_info) = builder.done();

        program.cfg.graph[node].instructions = instructions;
        program.cfg.graph[node].stack_info = stack_info;
    }

    fn apply_results(&mut self, _program: &mut Program) {}
}

pub struct BlockBuilder<'tbl> {
    state: State<'tbl>,
    instructions: Vec<Instruction>,
}

impl<'tbl> BlockBuilder<'tbl> {
    fn new(node: NodeIndex, symbol_table: &'tbl mut SymbolTable) -> Self {
        Self {
            state: State::new(node, symbol_table),
            instructions: Default::default(),
        }
    }

    fn done(self) -> (Vec<Instruction>, StackInfo) {
        let stack_info = StackInfo {
            arguments: self.state.borrows,
            generates: self.state.stack,
            height: self.state.height,
        };

        assert_eq!(
            stack_info.arguments as i32 + stack_info.height,
            stack_info.generates.len() as i32,
            "local stack elements must equal stack arguments taken + local height"
        );

        (self.instructions, stack_info)
    }

    fn translate(&mut self, opcode: &evmil::bytecode::Instruction) {
        use evmil::bytecode::Instruction::*;
        self.instructions.extend(match opcode {
            JUMPDEST => Vec::new(),

            PUSH(bytes) => {
                self.state.push(Symbol::builder().constant(bytes));

                Vec::new()
            }

            POP => {
                self.state.pop();

                Vec::new()
            }

            SWAP(n) => self.state.swap(*n as usize),

            DUP(n) => vec![Instruction::Copy {
                y: self.state.nth(*n as usize),
                x: self.state.push(Symbol::builder().variable()),
            }],

            ADD => vec![Instruction::BinaryAssign {
                y: self.state.pop(),
                z: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::Add,
            }],

            SUB => vec![Instruction::BinaryAssign {
                y: self.state.pop(),
                z: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::Sub,
            }],

            MSTORE => vec![Instruction::IndexedAssign {
                x: self.state.symbol_table.global(Global::Memory),
                index: self.state.pop(),
                y: self.state.pop(),
            }],

            MLOAD => vec![Instruction::IndexedCopy {
                index: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                y: self.state.symbol_table.global(Global::Memory),
            }],

            JUMP => vec![Instruction::UncoditionalBranch {
                target: self.state.pop(),
            }],

            JUMPI => vec![Instruction::ConditionalBranch {
                target: self.state.pop(),
                condition: self.state.pop(),
            }],

            CALLDATACOPY => vec![Instruction::Procedure {
                symbol: Global::CallDataCopy,
                parameters: vec![self.state.pop(), self.state.pop(), self.state.pop()],
            }],

            CALLDATALOAD => vec![Instruction::IndexedCopy {
                index: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                y: self.state.symbol_table.global(Global::CallData),
            }],

            RETURN => vec![Instruction::Procedure {
                symbol: Global::Return,
                parameters: vec![self.state.pop(), self.state.pop()],
            }],

            GT => vec![Instruction::BinaryAssign {
                y: self.state.pop(),
                z: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::GreaterThan,
            }],

            LT => vec![Instruction::BinaryAssign {
                y: self.state.pop(),
                z: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::LessThan,
            }],

            EQ => vec![Instruction::BinaryAssign {
                y: self.state.pop(),
                z: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::Equal,
            }],

            ISZERO => vec![Instruction::UnaryAssign {
                y: self.state.pop(),
                x: self.state.push(Symbol::builder().variable()),
                operator: Operator::IsZero,
            }],

            _ => {
                eprintln!("unimplement instruction: {opcode}");
                Vec::new()
            }
        })
    }
}

struct State<'tbl> {
    node: NodeIndex,
    symbol_table: &'tbl mut SymbolTable,
    stack: Vec<SymbolRef>,
    /// Every pop on an empty stack was counts as an additional argument.
    borrows: usize,
    /// Caches the arguments the block borrows from the stack.
    arguments: IndexMap<usize, SymbolRef>,
    /// Tracks the relative stack height:
    /// - Pushes increase the height by one
    /// - Pops decrease the height by one
    height: i32,
}

impl<'tbl> State<'tbl> {
    fn new(node: NodeIndex, symbol_table: &'tbl mut SymbolTable) -> Self {
        Self {
            node,
            symbol_table,
            stack: Default::default(),
            borrows: Default::default(),
            arguments: Default::default(),
            height: Default::default(),
        }
    }

    fn pop(&mut self) -> SymbolRef {
        self.height -= 1;
        self.stack.pop().unwrap_or_else(|| {
            self.borrows += 1;
            self.nth(0)
        })
    }

    fn push(&mut self, builder: SymbolBuilder<(), Kind>) -> SymbolRef {
        let symbol = builder.temporary().done();
        let symbol = self.symbol_table.insert(self.node, symbol);
        self.stack.push(symbol.clone());
        self.height += 1;

        symbol
    }

    fn swap(&mut self, n: usize) -> Vec<Instruction> {
        // For free if both elements are local to the basic block
        let top = self.stack.len().saturating_sub(1);
        if n <= top {
            self.stack.swap(top - n, top);
            return Vec::new();
        }

        let tmp = self.symbol_table.temporary(self.node);
        let a = self.nth(0);
        let b = self.nth(n);

        vec![
            Instruction::Copy {
                x: tmp.clone(),
                y: a.clone(),
            },
            Instruction::Copy { x: a, y: b.clone() },
            Instruction::Copy { x: b, y: tmp },
        ]
    }

    fn nth(&mut self, n: usize) -> SymbolRef {
        self.stack
            .iter()
            .rev()
            .nth(n)
            .or_else(|| self.arguments.get(&(self.slot(n) as usize)))
            .cloned()
            .unwrap_or_else(|| {
                let builder = Symbol::builder().stack(self.slot(n)).variable();
                let symbol = self.symbol_table.insert(self.node, builder.done());
                self.arguments.insert(self.slot(n) as usize, symbol.clone());
                symbol
            })
    }

    fn slot(&self, n: usize) -> i32 {
        n as i32 - (self.stack.len() as i32 - self.borrows as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockBuilder, State};
    use crate::{
        cfg::StackInfo,
        instruction::Instruction,
        symbol::{Symbol, SymbolTable},
    };
    use evmil::bytecode::Instruction::*;

    fn translate<'tbl>(code: &[evmil::bytecode::Instruction]) -> (Vec<Instruction>, StackInfo) {
        code.iter()
            .fold(
                BlockBuilder::new(Default::default(), &mut SymbolTable::default()),
                |mut builder, instruction| {
                    builder.translate(instruction);
                    builder
                },
            )
            .done()
    }

    #[test]
    fn stack_slot_works() {
        let mut symbol_table = SymbolTable::default();
        let mut state = State::new(Default::default(), &mut symbol_table);

        state.push(Symbol::builder().variable());
        assert_eq!(state.slot(0), -1);
        assert_eq!(state.slot(1), 0);
        assert_eq!(state.slot(2), 1);

        state.pop();
        state.pop();
        assert_eq!(state.slot(0), 1);
        assert_eq!(state.slot(1), 2);
        assert_eq!(state.slot(2), 3);

        state.push(Symbol::builder().variable());
        state.push(Symbol::builder().variable());
        assert_eq!(state.slot(0), -1);
        assert_eq!(state.slot(1), 0);
        assert_eq!(state.slot(2), 1);
    }

    #[test]
    fn push_works() {
        let state = translate(&[PUSH(vec![1])]).1;

        assert_eq!(state.height, 1);
        assert_eq!(state.arguments, 0);
        assert_eq!(state.generates.len(), 1);
    }

    #[test]
    fn add_works() {
        let state = translate(&[ADD]).1;

        assert_eq!(state.height, -1);
        assert_eq!(state.arguments, 2);
        assert_eq!(state.generates.len(), 1);
    }

    #[test]
    fn dup_works() {
        let state = translate(&[DUP(4)]).1;

        assert_eq!(state.height, 1);
        assert_eq!(state.arguments, 0);
        assert_eq!(state.generates.len(), 1);
    }

    #[test]
    fn swap_works() {
        let state = translate(&[SWAP(4)]).1;

        assert_eq!(state.height, 0);
        assert_eq!(state.arguments, 0);
        assert_eq!(state.generates.len(), 0);
    }

    #[test]
    fn jump() {
        let state = translate(&[JUMP]).1;

        assert_eq!(state.height, -1);
        assert_eq!(state.arguments, 1);
        assert_eq!(state.generates.len(), 0);
    }

    #[test]
    fn pop5_push2() {
        let state = translate(&[POP, POP, POP, POP, POP, PUSH(vec![1]), PUSH(vec![1])]).1;

        assert_eq!(state.height, -3);
        assert_eq!(state.arguments, 5);
        assert_eq!(state.generates.len(), 2);
    }

    #[test]
    fn fibonacci_loop_body() {
        let state = translate(&[
            PUSH(vec![1]),
            ADD,
            SWAP(2),
            DUP(1),
            SWAP(4),
            ADD,
            SWAP(2),
            PUSH(vec![10]),
            JUMP,
        ])
        .1;

        assert_eq!(state.height, 0);
        assert_eq!(state.arguments, 1);
        assert_eq!(state.generates.len(), 1);
    }
}
