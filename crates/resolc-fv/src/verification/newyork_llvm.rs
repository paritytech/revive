//! The obligation codegen carries.

use revive_newyork::ir;

use crate::common::Proof;
use crate::verification::simulation::simulated_proof;
use crate::verification::smt;
use crate::verification::smt::Solver;

/// Whether the LLVM IR means what the newyork IR it was lowered from meant.
///
/// Not implemented. This is the obligation codegen carries, the last link in the
/// chain and the second of the two that cross formats.
pub fn is_equivalent(_newyork: &ir::Object, _llvm: &str, solver: &Solver) -> Proof {
    simulated_proof("the newyork to LLVM codegen check").with_note(smt::probe(solver))
}
