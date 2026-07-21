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
pub mod source;
pub mod stale;
/// Input-validity funnel stats; only used by the libFuzzer targets.
#[cfg(feature = "panic-on-divergence")]
pub mod stats;
pub mod templates;

pub use differential::{run_case_solc_evm, CompareReport, Divergence};
pub use generator::{Action, SolidityCase};
pub use observe::{ActionResult, Outcome};
pub use source::extract_entrypoint_name;
pub use stale::warn_if_resolc_stale;

/// Surface divergences as panics so libFuzzer saves the input as a crash
/// artifact. Before aborting, the reproducer is also written to a durable
/// findings directory (see [`REVIVE_FUZZ_FINDINGS_DIR`]) because libFuzzer's
/// fork mode does not reliably persist crash artifacts. The panic message
/// embeds the rendered source + action sequence so the log alone can reproduce.
#[cfg(feature = "panic-on-divergence")]
pub mod panic_on_divergence {
    use std::fmt::Write;

    use alloy_primitives::keccak256;

    use crate::{run_case_solc_evm, warn_if_resolc_stale, SolidityCase};

    /// Env var selecting where findings are written. Prefer an absolute path:
    /// libFuzzer fork workers may run from a temp directory. Defaults to
    /// [`DEFAULT_FINDINGS_DIR`] (relative to the worker's CWD).
    pub const REVIVE_FUZZ_FINDINGS_DIR: &str = "REVIVE_FUZZ_FINDINGS_DIR";

    /// Fallback findings directory when the env var is unset.
    const DEFAULT_FINDINGS_DIR: &str = "findings";

    /// Bytes of the finding digest used for the content-addressed filename.
    const FINDING_KEY_BYTES: usize = 16;

    /// Direct-solc EVM path keeps revive-yul printer bugs out of the
    /// noise floor. `EvmCompile` (solc rejected the template) is a
    /// generator bug → silently skipped.
    pub fn run_solidity_case_panic(case: &SolidityCase) {
        warn_if_resolc_stale();
        crate::stats::record_decoded();
        let result = run_case_solc_evm(case);
        // Funnel stats: solc accepted unless it rejected the source
        // (`EvmCompile`); resolc also compiled when the comparison ran (`Ok`).
        if !matches!(&result, Err(crate::Divergence::EvmCompile(_))) {
            crate::stats::record_solc_ok();
        }
        if result.is_ok() {
            crate::stats::record_resolc_ok();
        }
        if let Err(
            crate::Divergence::EvmCompile(_)
            | crate::Divergence::PvmResourceLimit(_)
            | crate::Divergence::EvmResourceLimit(_),
        ) = &result
        {
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
            let detail = format!(
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
            // Persist the reproducer before aborting: libFuzzer fork mode does
            // not reliably save crash artifacts, so each worker saves its own.
            persist_finding(&case.source, &detail);
            panic!("{detail}");
        }
    }

    /// Configured findings directory (env override, else [`DEFAULT_FINDINGS_DIR`]).
    fn findings_dir() -> String {
        std::env::var(REVIVE_FUZZ_FINDINGS_DIR).unwrap_or_else(|_| DEFAULT_FINDINGS_DIR.to_owned())
    }

    /// Content-addressed filename stem for a finding.
    fn finding_key(detail: &str) -> String {
        hex::encode(&keccak256(detail.as_bytes())[..FINDING_KEY_BYTES])
    }

    /// Write `<key>.sol` (the contract, directly reproducible) and `<key>.txt`
    /// (full divergence detail) into `dir`. Best-effort; returns the `.sol` path
    /// on success. Content-addressed, so duplicate findings collapse.
    fn write_finding(dir: &str, source: &str, detail: &str) -> Option<String> {
        std::fs::create_dir_all(dir).ok()?;
        let key = finding_key(detail);
        let sol_path = format!("{dir}/{key}.sol");
        std::fs::write(&sol_path, source).ok()?;
        std::fs::write(format!("{dir}/{key}.txt"), detail).ok()?;
        Some(sol_path)
    }

    /// Persist a finding to the configured directory, logging where it landed.
    fn persist_finding(source: &str, detail: &str) {
        if let Some(path) = write_finding(&findings_dir(), source, detail) {
            eprintln!("[revive-fuzz] finding saved: {path}");
        }
    }

    /// Source-text variant: the bytes are Solidity source (e.g. an
    /// LLM-generated contract seeded into the corpus). Builds a
    /// fixed-wire-shape case via [`SolidityCase::from_source`] and defers to
    /// [`run_solidity_case_panic`]. Inputs with no `fn_0` entrypoint — e.g. a
    /// mutation that broke the contract — are silently skipped.
    pub fn run_source_case_panic(source: &str) {
        if let Some(case) = SolidityCase::from_source(source) {
            run_solidity_case_panic(&case);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn writes_content_addressed_reproducer() {
            let dir =
                std::env::temp_dir().join(format!("revive-fuzz-findings-{}", std::process::id()));
            let dir = dir.to_str().expect("utf-8 temp path");
            let source = "// SPDX-License-Identifier: MIT\ncontract Fuzz {}";
            let detail = "divergence: action[0] revert mismatch\nsource:\n...";

            let sol = write_finding(dir, source, detail).expect("finding written");
            assert!(sol.ends_with(".sol"));
            assert_eq!(std::fs::read_to_string(&sol).unwrap(), source);
            let txt = sol.strip_suffix(".sol").unwrap().to_owned() + ".txt";
            assert_eq!(std::fs::read_to_string(txt).unwrap(), detail);

            // Same detail → same key (idempotent, de-duplicating).
            assert_eq!(
                write_finding(dir, source, detail).as_deref(),
                Some(sol.as_str())
            );

            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
