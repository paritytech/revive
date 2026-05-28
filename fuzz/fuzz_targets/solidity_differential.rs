//! libFuzzer entry: Solidity direct-solc → EVM vs resolc → PVM.
//!
//! Each iteration: bytes → [`SolidityCase`] → both backends →
//! compare. Divergence panics; libFuzzer writes the bytes to
//! `fuzz/artifacts/solidity_differential/crash-*`.
//!
//! cargo-fuzz instruments every Rust crate in the dep graph with
//! SanitizerCoverage, so the mutation engine sees edges in
//! revive-yul / resolc / revive-llvm-context / revive-runner.
//! `solc` and `evm` subprocesses are opaque. Note also that the
//! recursive resolc subprocess spawned via `EXECUTABLE` in
//! `resolc::test_utils` (the installed `~/.cargo/bin/resolc`) is
//! NOT instrumented either — only the in-process call sites are.

#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use revive_fuzz::panic_on_divergence::run_solidity_case_panic;
use revive_fuzz::SolidityCase;

fuzz_target!(|data: &[u8]| {
    let mut unstructured = Unstructured::new(data);
    let Ok(case) = SolidityCase::arbitrary(&mut unstructured) else {
        return;
    };
    run_solidity_case_panic(&case);
});
