use evmil::bytecode::Disassemble;
use revive_ir::cfg::BasicBlockFormatOption;
use revive_target_polkavm::PolkaVm;

fn main() {
    let hexcode = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let bytecode = hex::decode(hexcode.trim()).unwrap();
    let instructions = bytecode.disassemble();

    let mut ir = revive_ir::cfg::Program::new(&instructions);
    ir.optimize();
    ir.dot(BasicBlockFormatOption::Ir);

    let target = PolkaVm::default();
    let program = revive_codegen::program::Program::new(&target).unwrap();
    program.emit(ir);

    let artifact = program.compile_and_link();

    std::fs::write("/tmp/out.pvm", artifact).unwrap();
}
