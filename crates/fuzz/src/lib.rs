//! libFuzzer-driven differential fuzzer.
//!
//! For each [`SolidityCase`]: PVM via `resolc → revive-runner`, EVM
//! via [`run_case_solc_evm`] (direct solc — pure backend-vs-backend).
//! Mismatch on `(deploy_reverted, per-action reverted, return_data)`
//! → [`Divergence`]. The libfuzzer-sys target under `fuzz/` uses
//! [`panic_on_divergence::run_solidity_case_panic`].

pub mod differential;
pub mod generator;
pub mod observe;
pub mod pipeline;
pub mod stale;
pub mod templates;

pub use differential::{run_case_solc_evm, CompareReport, Divergence};
pub use generator::{Action, SolidityCase};
pub use observe::{ActionResult, Outcome};
pub use stale::warn_if_resolc_stale;

/// Surface divergences as panics so libFuzzer saves the input as
/// a crash artifact. The panic message embeds the rendered source
/// + action sequence so the crash + log are enough to reproduce.
#[cfg(feature = "panic-on-divergence")]
pub mod panic_on_divergence {
    use std::fmt::Write;

    use crate::{run_case_solc_evm, warn_if_resolc_stale, SolidityCase};

    /// Direct-solc EVM path keeps revive-yul printer bugs out of the
    /// noise floor. `EvmCompile` (solc rejected the template) is a
    /// generator bug → silently skipped.
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
}
