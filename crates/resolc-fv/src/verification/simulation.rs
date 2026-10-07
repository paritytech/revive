//! A stand-in for the solvers, until there are solvers.
//!
//! Everything here goes when the real checks arrive.

use std::ops::Range;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use crate::common::CounterExample;
use crate::common::Proof;
use crate::common::Verdict;

/// How long a simulated solver run takes, in milliseconds.
///
/// Wide enough that obligations visibly finish out of order under `--jobs`,
/// which is the behaviour worth being able to see.
const SIMULATED_SOLVER_MILLIS: Range<u64> = 150..2_500;

/// How often a simulated run comes back proved, out of a hundred.
const SIMULATED_PROVED_PERCENT: u64 = 70;

/// How often a simulated run comes back refuted, out of a hundred.
///
/// The rest come back unknown, which is what a solver that times out does.
const SIMULATED_REFUTED_PERCENT: u64 = 10;

/// Stands in for a solver run that `check` does not yet perform.
///
/// Sleeps for a random time and decides at random, so that the rest of the tool
/// can be exercised against something that behaves like a solver. Goes when the
/// real checks arrive, along with the randomness below it.
pub fn simulated_proof(check: &str) -> Proof {
    let span = SIMULATED_SOLVER_MILLIS.end - SIMULATED_SOLVER_MILLIS.start;
    let millis = SIMULATED_SOLVER_MILLIS.start + simulation_random() % span;
    let started = Instant::now();
    std::thread::sleep(Duration::from_millis(millis));

    let transcript = format!(
        "check: {check}\nsolver: simulated\nelapsed: {} ms\n",
        started.elapsed().as_millis(),
    );

    match simulation_random() % 100 {
        roll if roll < SIMULATED_PROVED_PERCENT => Proof {
            verdict: Verdict::Equivalent,
            log: format!("{transcript}result: proved\n"),
        },
        roll if roll < SIMULATED_PROVED_PERCENT + SIMULATED_REFUTED_PERCENT => {
            let counterexample = CounterExample {
                description: format!("simulated counterexample from {check}"),
            };
            Proof {
                log: format!("{transcript}result: refuted\ncounterexample: {counterexample}\n"),
                verdict: Verdict::NotEquivalent(counterexample),
            }
        }
        _ => {
            let reason = format!("the simulated solver for {check} gave up");
            Proof {
                log: format!("{transcript}result: unknown\nreason: {reason}\n"),
                verdict: Verdict::Unproven(reason),
            }
        }
    }
}

/// A random number for the simulated solver runs.
///
/// Nothing here needs statistical quality, only variety across runs and across
/// threads, so it is not worth a dependency: a counter put through the
/// splitmix64 finalizer gives both, and does it without locking.
fn simulation_random() -> u64 {
    /// Varies the sequence between runs. The standard hasher is randomly keyed
    /// per process, which is the entropy the standard library already has.
    static SEED: OnceLock<u64> = OnceLock::new();
    /// Advanced once per call, so concurrent provers never share a draw.
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// The golden-ratio odd constant splitmix64 strides the counter by.
    const STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

    let seed = *SEED.get_or_init(|| {
        use std::hash::BuildHasher;
        use std::hash::Hasher;

        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    });

    let mut value = seed.wrapping_add(COUNTER.fetch_add(STRIDE, Ordering::Relaxed));
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);

    value ^ (value >> 31)
}
