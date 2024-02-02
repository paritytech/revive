use crate::{
    analysis::{
        analyze, control_flow::StaticJumps, evm_bytecode::IrBuilder, types::TypePropagation,
    },
    cfg::Program,
};

use super::Pass;

#[derive(Default)]
pub struct BytecodeLifter;

impl Pass for BytecodeLifter {
    fn run(&mut self, program: &mut Program) {
        analyze::<IrBuilder>(program);
        analyze::<StaticJumps>(program);
        analyze::<TypePropagation>(program);
    }
}
