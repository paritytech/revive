//! Writing out what a prover concluded.

use std::path::Path;
use std::path::PathBuf;

use crate::common::Chain;
use crate::common::Obligation;
use crate::common::Proof;
use crate::common::Verdict;

/// The extension of a proof log.
pub const LOG_EXTENSION: &str = "fv.log";

/// Writes one proof's log, and returns where it went.
///
/// The log carries the verdict and the counterexample in its own right rather
/// than relying on the checker's transcript to mention them, so that a log is
/// readable whatever a future checker chooses to record.
pub fn write_log(
    log_directory: &Path,
    chain: &Chain,
    obligation: &Obligation<'_>,
    proof: &Proof,
) -> anyhow::Result<PathBuf> {
    let path = log_directory.join(format!(
        "{}.{}.{LOG_EXTENSION}",
        chain.stem(),
        obligation.label,
    ));

    let mut contents = format!(
        "step: {} -> {}\ntransform: {} -> {}\nverdict: {}\n",
        obligation.source,
        obligation.target,
        obligation.from,
        obligation.to,
        proof.verdict.name(),
    );
    match &proof.verdict {
        Verdict::Equivalent => {}
        Verdict::NotEquivalent(counterexample) => {
            contents.push_str(&format!("counterexample: {counterexample}\n"));
        }
        Verdict::Unproven(reason) => {
            contents.push_str(&format!("reason: {reason}\n"));
        }
    }
    contents.push_str("---\n");
    contents.push_str(&proof.log);

    std::fs::write(&path, contents)
        .map_err(|error| anyhow::anyhow!("writing the proof log {}: {error}", path.display()))?;

    Ok(path)
}
