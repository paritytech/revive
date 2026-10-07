//! The obligation every optimization pass carries.

use revive_newyork::ir;

use crate::common::Proof;
use crate::verification::simulation::simulated_proof;
use crate::verification::smt;
use crate::verification::smt::Solver;

/// Whether an optimization pass preserved the meaning of the IR it rewrote.
///
/// Not implemented. This is the obligation every optimization pass carries.
pub fn is_equivalent(_before: &ir::Object, _after: &ir::Object, solver: &Solver) -> Proof {
    simulated_proof("the newyork pass equivalence check").with_note(smt::probe(solver))
}
