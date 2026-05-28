//! Random-seeded driver for the Yul differential fuzzer.
//!
//! Same shape as `revive_fuzz.rs`, but generates **Yul source**. Both
//! backends consume identical text (`solc --strict-assembly` and
//! resolc's Yul-input pipeline) so a divergence is a pure backend
//! mismatch — no Solidity-frontend variable to control for.

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use arbitrary::{Arbitrary, Unstructured};
use clap::Parser;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;
use revive_fuzz::{run_yul_case, YulCase, YulCompareReport, YulDivergence};

/// Print one progress dot every this many completed iterations.
const PROGRESS_DOT_EVERY: u64 = 10;

#[derive(Parser, Debug)]
#[command(name = "revive-yul-fuzz", about = "Yul-based EVM-vs-PVM differential fuzzer")]
struct Args {
    /// Number of cases to run. `0` means "loop forever".
    #[arg(short = 'n', long, default_value_t = 100)]
    iterations: u64,
    /// PRNG seed. Defaults to a fresh OS-provided seed.
    #[arg(short = 's', long)]
    seed: Option<u64>,
    /// Stop on the first divergence instead of accumulating.
    #[arg(long, default_value_t = false)]
    stop_on_divergence: bool,
    /// Bytes of `arbitrary` input per case.
    #[arg(long, default_value_t = 4096)]
    input_size: usize,
    /// Print both observations for every iteration regardless of
    /// match/mismatch.
    #[arg(long, default_value_t = false)]
    verbose: bool,
    /// Number of worker threads. Defaults to the logical CPU count.
    /// Each iteration is deterministically seeded from `(seed,
    /// iteration_index)`.
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Treat solc rejecting a generated case as a divergence (default:
    /// silently skip). Solc rejecting templates is a generator bug,
    /// not a backend bug — same default as `revive-fuzz`.
    #[arg(long, default_value_t = false)]
    strict_template_errors: bool,
}

fn main() -> ExitCode {
    // Silence PVM toolchain noise — same set as revive-fuzz.
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .filter_module("polkavm", log::LevelFilter::Error)
        .filter_module("polkavm_linker", log::LevelFilter::Error)
        .filter_module("polkavm_common", log::LevelFilter::Error)
        .filter_module("pallet_revive", log::LevelFilter::Error)
        .init();
    let args = Args::parse();

    let seed = args.seed.unwrap_or_else(rand::random);
    let threads = args.threads.unwrap_or_else(num_cpus::get).max(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("rayon thread pool init");
    log::info!(
        "revive-yul-fuzz seed={seed} iterations={} threads={threads}",
        args.iterations
    );

    let target = if args.iterations == 0 {
        u64::MAX
    } else {
        args.iterations
    };

    let divergences = AtomicU64::new(0);
    let stop = AtomicBool::new(false);
    let completed = AtomicU64::new(0);
    let print_lock = Mutex::new(());

    const CHUNK: u64 = 1024;
    let mut start: u64 = 0;
    while start < target && !stop.load(Ordering::Relaxed) {
        let end = start.saturating_add(CHUNK).min(target);
        (start..end).into_par_iter().for_each(|iter| {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            run_iteration(
                iter,
                seed,
                args.input_size,
                args.verbose,
                args.stop_on_divergence,
                args.strict_template_errors,
                &divergences,
                &completed,
                &stop,
                &print_lock,
            );
        });
        start = end;
    }

    if completed.load(Ordering::Relaxed) >= PROGRESS_DOT_EVERY {
        eprintln!();
    }
    let final_count = divergences.load(Ordering::Relaxed);
    log::info!("done. divergences={final_count}");
    match (args.stop_on_divergence, final_count) {
        (true, n) if n > 0 => ExitCode::from(2),
        (_, 0) => ExitCode::SUCCESS,
        _ => ExitCode::from(1),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_iteration(
    iter: u64,
    seed: u64,
    input_size: usize,
    verbose: bool,
    stop_on_divergence: bool,
    strict_template_errors: bool,
    divergences: &AtomicU64,
    completed: &AtomicU64,
    stop: &AtomicBool,
    print_lock: &Mutex<()>,
) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed.wrapping_add(iter));
    let mut buffer = vec![0u8; input_size];
    rng.fill_bytes(&mut buffer);
    let mut unstructured = Unstructured::new(&buffer);
    let case = match YulCase::arbitrary(&mut unstructured) {
        Ok(case) => case,
        Err(error) => {
            log::debug!("iter {iter}: arbitrary exhausted: {error}");
            return;
        }
    };

    let result = run_yul_case(&case);
    if !strict_template_errors && matches!(&result, Err(YulDivergence::EvmCompile(_))) {
        log::debug!("iter {iter}: yul template rejected by solc, skipping");
    } else {
        match result {
            Ok(report) => {
                if verbose {
                    let _guard = print_lock.lock().expect("print_lock poisoned");
                    print_observations(iter, &case, &report);
                } else {
                    log::debug!("iter {iter}: ok ({})", case.contract_name);
                }
            }
            Err(divergence) => {
                divergences.fetch_add(1, Ordering::Relaxed);
                let _guard = print_lock.lock().expect("print_lock poisoned");
                print_divergence(iter, seed, &case, &divergence);
                if stop_on_divergence {
                    stop.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    let prev = completed.fetch_add(1, Ordering::Relaxed);
    if (prev + 1) % PROGRESS_DOT_EVERY == 0 {
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, ".");
        let _ = stderr.flush();
    }
}

fn print_observations(iter: u64, case: &YulCase, report: &YulCompareReport) {
    println!("--- iter {iter} {} ---", case.contract_name);
    println!(
        "deploy_reverted: evm={}, pvm={}",
        report.evm.deploy_reverted, report.pvm.deploy_reverted
    );
    for (i, (a, b)) in report.evm.actions.iter().zip(report.pvm.actions.iter()).enumerate() {
        let calldata = case
            .actions
            .get(i)
            .map(hex::encode)
            .unwrap_or_default();
        println!(
            "  action[{i}] calldata=0x{calldata}\n    evm: revert={} ret=0x{}\n    pvm: revert={} ret=0x{}",
            a.reverted,
            hex::encode(&a.return_data),
            b.reverted,
            hex::encode(&b.return_data)
        );
    }
}

fn print_divergence(iter: u64, seed: u64, case: &YulCase, divergence: &YulDivergence) {
    println!("---");
    println!("# revive-yul-fuzz divergence");
    println!("seed: {seed}");
    println!("iteration: {iter}");
    println!("contract: {}", case.contract_name);
    println!("divergence: {divergence}");
    print_divergence_values(divergence);
    println!("source:");
    println!("{}", case.source);
    println!("actions ({}):", case.actions.len());
    for (i, calldata) in case.actions.iter().enumerate() {
        println!("  [{i}] 0x{}", hex::encode(calldata));
    }
    println!("---");
}

fn print_divergence_values(divergence: &YulDivergence) {
    match divergence {
        YulDivergence::EvmCompile(_) | YulDivergence::PvmCompile(_) => {}
        YulDivergence::DeployRevert { evm, pvm } => {
            println!("values:");
            println!("  evm deploy_reverted: {evm}");
            println!("  pvm deploy_reverted: {pvm}");
        }
        YulDivergence::ActionCount { evm, pvm } => {
            println!("values:");
            println!("  evm action count: {evm}");
            println!("  pvm action count: {pvm}");
        }
        YulDivergence::ActionRevert { index, evm, pvm } => {
            println!("values:");
            println!("  action[{index}] evm reverted: {evm}");
            println!("  action[{index}] pvm reverted: {pvm}");
        }
        YulDivergence::ActionReturnData { index, full, .. } => {
            let (a, b) = full.as_ref();
            println!("values:");
            println!("  action[{index}] evm return: 0x{}", hex::encode(a));
            println!("  action[{index}] pvm return: 0x{}", hex::encode(b));
        }
    }
}
