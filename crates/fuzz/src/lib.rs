//! Differential fuzzer for the revive compiler.
//!
//! Given a generated [`SolidityCase`], the harness produces two
//! observations of the same logical contract execution: PVM side
//! via `resolc → revive-runner`, EVM side via one of two paths:
//!
//! * [`run_case`] — **revive-yul roundtrip**: solc emits Yul,
//!   [`revive_yul`] reprints it via
//!   [`Printer`](revive_yul::visitor::Printer), `solc
//!   --strict-assembly` compiles the reprint to EVM. Stresses
//!   revive-yul's printer.
//! * [`run_case_solc_evm`] — **direct solc**: solc compiles to EVM
//!   bytecode directly. Pure backend-vs-backend.
//!
//! Mismatch on (`deploy_reverted`, per-action `reverted`,
//! per-action `return_data`) is reported as a [`Divergence`].
//!
//! Drivers:
//! * `revive-fuzz` binary (random-seeded loop, both EVM paths)
//! * `revive-yul-fuzz` binary (Yul-source generator,
//!   [`run_yul_case`])
//! * libfuzzer-sys target under `fuzz/` (uses [`run_case_solc_evm`]
//!   via [`panic_on_divergence::run_solidity_case_panic`])

pub mod differential;
pub mod generator;
pub mod observe;
pub(crate) mod panic_util;
pub mod pipeline;
pub mod stale;
pub mod templates;
pub mod yul;

pub use differential::{run_case, run_case_solc_evm, CompareReport, Divergence};
pub use generator::{Action, SolidityCase};
pub use observe::{ActionResult, Outcome};
pub use stale::warn_if_resolc_stale;
pub use yul::{run_yul_case, YulCase, YulCompareReport, YulDivergence};

/// libFuzzer-shaped wrappers: surface divergences as panics so
/// libFuzzer saves the input bytes under
/// `fuzz/artifacts/<target>/crash-*`. The panic message embeds the
/// rendered source + action sequence so a crash artifact plus the
/// panic log are enough to reproduce.
#[cfg(feature = "panic-on-divergence")]
pub mod panic_on_divergence {
    use std::fmt::Write;

    use crate::{run_case_solc_evm, run_yul_case, warn_if_resolc_stale, SolidityCase, YulCase};

    /// Direct-solc EVM path so revive-yul printer bugs don't pollute
    /// findings. `EvmCompile` (solc rejected the template) is treated
    /// as a generator bug and silently skipped instead of crashing.
    pub fn run_solidity_case_panic(case: &SolidityCase) {
        warn_if_resolc_stale();
        let result = run_case_solc_evm(case);
        if let Err(crate::Divergence::EvmCompile(_)) = &result {
            return;
        }
        if let Err(divergence) = result {
            let mut args = String::new();
            for (i, arg) in case.constructor_args.iter().enumerate() {
                let _ = writeln!(args, "  [{i}] 0x{}", hex::encode(arg));
            }
            let mut actions = String::new();
            for (i, action) in case.actions.iter().enumerate() {
                let _ = writeln!(actions, "  [{i}] fn_0(0x{})", hex::encode(action.argument));
            }
            panic!(
                "solidity differential divergence: {divergence}\n\
                 contract: {}\n\
                 source:\n{}\n\
                 constructor_args ({}):\n{}\
                 actions ({}):\n{}",
                case.contract_name,
                case.source,
                case.constructor_args.len(),
                args,
                case.actions.len(),
                actions,
            );
        }
    }

    /// Same skip-on-generator-bug rule as `run_solidity_case_panic`:
    /// `EvmCompile` (solc-strict-assembly rejecting the Yul) is
    /// treated as a generator bug and silently skipped. `PvmCompile`
    /// (resolc-yul-input rejecting solc-accepted Yul) crashes.
    ///
    /// Treating solc-strict-assembly as the reference is a choice,
    /// not a property of the Yul spec — both parsers are
    /// independent. If resolc's Yul parser is meaningfully stricter
    /// than solc's, this asymmetry would surface as a stream of
    /// `PvmCompile` crashes that are really generator-permissiveness
    /// issues. Revisit when wiring up a libFuzzer Yul target.
    pub fn run_yul_case_panic(case: &YulCase) {
        warn_if_resolc_stale();
        let result = run_yul_case(case);
        if let Err(crate::YulDivergence::EvmCompile(_)) = &result {
            return;
        }
        if let Err(divergence) = result {
            let mut actions = String::new();
            for (i, calldata) in case.actions.iter().enumerate() {
                let _ = writeln!(actions, "  [{i}] 0x{}", hex::encode(calldata));
            }
            panic!(
                "yul differential divergence: {divergence}\n\
                 contract: {}\n\
                 source:\n{}\n\
                 actions ({}):\n{}",
                case.contract_name,
                case.source,
                case.actions.len(),
                actions,
            );
        }
    }
}
