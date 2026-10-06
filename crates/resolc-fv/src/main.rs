//! Formal verification tooling for the newyork IR.
//!
//! `resolc` emits one artifact per optimization pass under `--emit-ir-stages`,
//! indexed by a manifest. This binary consumes that chain and discharges one
//! equivalence obligation per adjacent pair of stages, so that the optimizer's
//! output can be checked against its input without trusting the optimizer.
//!
//! The schema of both the manifest and the stage artifacts lives in
//! `revive_newyork::export`, shared with the compiler that writes them.
//!
//! This is a scaffold: the tool does not read any artifacts yet.

fn main() {
    println!("resolc-fv: formal verification for the newyork IR (scaffold).");
}
