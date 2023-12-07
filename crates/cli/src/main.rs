use evmil::bytecode::Disassemble;
use ir_tac::cfg::{BasicBlockFormatOption, Program};

fn main() {
    let hexcode = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let bytecode = hex::decode(hexcode.trim()).unwrap();
    let instructions = bytecode.disassemble();

    Program::new(instructions).dot(BasicBlockFormatOption::Ir);
}
