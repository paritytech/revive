//! Input-validity statistics for the libFuzzer targets.
//!
//! Tracks how far each raw fuzzer input gets through the funnel:
//!
//! ```text
//! inputs  → decoded    → solc_ok        → resolc_ok
//! (raw)     (a case)     (valid Solidity) (both backends compiled)
//! ```
//!
//! Counters are process-global atomics (each `-fork` worker is its own
//! process). A one-line summary is emitted to stderr every
//! `REVIVE_FUZZ_STATS_INTERVAL` inputs (default [`DEFAULT_INTERVAL`]; `0`
//! disables), so you can see what fraction of mutated bytes become real cases.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static INPUTS: AtomicU64 = AtomicU64::new(0);
static DECODED: AtomicU64 = AtomicU64::new(0);
static SOLC_OK: AtomicU64 = AtomicU64::new(0);
static RESOLC_OK: AtomicU64 = AtomicU64::new(0);

/// Inputs between summary lines when the interval env var is unset.
const DEFAULT_INTERVAL: u64 = 1000;

/// Summary cadence in inputs; `0` disables output. Read once from
/// `REVIVE_FUZZ_STATS_INTERVAL`.
fn interval() -> u64 {
    static INTERVAL: OnceLock<u64> = OnceLock::new();
    *INTERVAL.get_or_init(|| {
        std::env::var("REVIVE_FUZZ_STATS_INTERVAL")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_INTERVAL)
    })
}

/// Count one raw input from the fuzzer, emitting a summary at the interval.
pub fn record_input() {
    let inputs = INPUTS.fetch_add(1, Ordering::Relaxed) + 1;
    let step = interval();
    if step != 0 && inputs.is_multiple_of(step) {
        emit(inputs);
    }
}

/// The raw bytes decoded into a runnable [`crate::SolidityCase`].
pub(crate) fn record_decoded() {
    DECODED.fetch_add(1, Ordering::Relaxed);
}

/// solc accepted the source (it is valid Solidity).
pub(crate) fn record_solc_ok() {
    SOLC_OK.fetch_add(1, Ordering::Relaxed);
}

/// resolc also compiled it — the full differential ran.
pub(crate) fn record_resolc_ok() {
    RESOLC_OK.fetch_add(1, Ordering::Relaxed);
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn emit(inputs: u64) {
    let decoded = DECODED.load(Ordering::Relaxed);
    let solc_ok = SOLC_OK.load(Ordering::Relaxed);
    let resolc_ok = RESOLC_OK.load(Ordering::Relaxed);
    eprintln!(
        "[revive-fuzz pid={}] inputs={inputs} \
         decoded={decoded} ({:.1}%) \
         solc_ok={solc_ok} ({:.1}%) \
         resolc_ok={resolc_ok} ({:.1}%)",
        std::process::id(),
        percent(decoded, inputs),
        percent(solc_ok, inputs),
        percent(resolc_ok, inputs),
    );
}
