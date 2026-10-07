//! The obligation the Yul translation carries.

use revive_newyork::ir;
use revive_yul::parser::statement::object;

use crate::common::Proof;
use crate::verification::simulation::simulated_proof;
use crate::verification::smt;
use crate::verification::smt::Solver;

/// Whether the newyork IR means what the Yul AST it was translated from meant.
///
/// Not implemented. This is the obligation for the `from_yul` translation.
pub fn is_equivalent(_yul: &object::Object, _newyork: &ir::Object, solver: &Solver) -> Proof {
    simulated_proof("the Yul to newyork translation check").with_note(smt::probe(solver))
}
