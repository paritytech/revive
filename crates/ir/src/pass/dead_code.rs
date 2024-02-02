use crate::{
    analysis::{analyze, control_flow::ReachableCode},
    cfg::Program,
};

use super::Pass;

#[derive(Default)]
pub struct DeadCodeElimination;

impl Pass for DeadCodeElimination {
    fn run(&mut self, program: &mut Program) {
        analyze::<ReachableCode>(program);
    }
}
