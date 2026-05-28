//! Random-seeded loop calling [`run_case`] / [`run_case_solc_evm`].
//! Local + CI driver; the libFuzzer target in `fuzz/` shares the
//! same entry points.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Print one progress dot every this many completed iterations.
const PROGRESS_DOT_EVERY: u64 = 10;

use arbitrary::{Arbitrary, Unstructured};
use clap::Parser;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;
use revive_fuzz::{
    run_case, run_case_solc_evm, warn_if_resolc_stale, CompareReport, Divergence, SolidityCase,
};

#[derive(Parser, Debug)]
#[command(name = "revive-fuzz", about = "Differential fuzzer (revive-Yul/solc → EVM vs resolc → PVM)")]
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
    /// Bytes of `arbitrary` input per case. Larger gives the generator
    /// more freedom; smaller speeds up the inner loop.
    #[arg(long, default_value_t = 4096)]
    input_size: usize,
    /// Print both observations for every iteration regardless of
    /// match/mismatch — used to debug "fuzzer says clean but I know
    /// the compiler is buggy".
    #[arg(long, default_value_t = false)]
    verbose: bool,
    /// Number of worker threads. Defaults to the logical CPU count.
    /// Each iteration is independent and deterministically seeded
    /// from `(seed, iteration_index)`, so a divergence found at
    /// parallelism N reproduces at parallelism 1 with the same seed.
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// When set, switch from differential mode to **emit mode**: each
    /// generated Solidity contract is written to
    /// `<output-dir>/iter_<N>_<contract_name>.sol`; no compilation
    /// and no comparison are performed. Useful for building a corpus
    /// of valid Solidity for downstream tools.
    #[arg(long, value_name = "DIR")]
    output_dir: Option<PathBuf>,
    /// Use the direct `solc → EVM` path instead of the default
    /// `revive-yul-roundtrip → solc-strict-asm → EVM` path. Direct
    /// solc strips revive-yul printer bugs from the noise floor —
    /// pick this when the goal is pure backend-vs-backend findings.
    #[arg(long, default_value_t = false)]
    direct_solc_evm: bool,
    /// Treat solc rejecting a generated case as a divergence (default:
    /// silently skip). `solc` rejecting templates is a generator bug,
    /// not a backend bug; matching libFuzzer's default keeps the
    /// signal:noise ratio sane. Enable when triaging template churn.
    #[arg(long, default_value_t = false)]
    strict_template_errors: bool,
}

fn main() -> ExitCode {
    // PVM toolchain emits DWARF/linker warnings per compile — silence
    // them so divergence reports stand out. `RUST_LOG` still wins.
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .filter_module("polkavm", log::LevelFilter::Error)
        .filter_module("polkavm_linker", log::LevelFilter::Error)
        .filter_module("polkavm_common", log::LevelFilter::Error)
        .filter_module("pallet_revive", log::LevelFilter::Error)
        .init();
    warn_if_resolc_stale();
    let args = Args::parse();

    let seed = args.seed.unwrap_or_else(|| rand::random());
    let threads = args.threads.unwrap_or_else(num_cpus::get).max(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("rayon thread pool init");
    let mode = if let Some(dir) = args.output_dir.as_ref() {
        if let Err(error) = std::fs::create_dir_all(dir) {
            log::error!("could not create --output-dir {}: {error}", dir.display());
            return ExitCode::from(2);
        }
        log::info!(
            "revive-fuzz seed={seed} iterations={} threads={threads} emit-dir={}",
            args.iterations,
            dir.display(),
        );
        Mode::Emit
    } else {
        log::info!(
            "revive-fuzz seed={seed} iterations={} threads={threads}",
            args.iterations
        );
        Mode::Differential
    };

    let target = if args.iterations == 0 {
        u64::MAX
    } else {
        args.iterations
    };

    let divergences = AtomicU64::new(0);
    let stop = AtomicBool::new(false);
    let completed = AtomicU64::new(0);
    // Stdout lock so multi-line divergence reports don't interleave.
    let print_lock = Mutex::new(());

    // Bounded chunks: lets --stop-on-divergence cut short promptly,
    // and bounds the rayon graph when --iterations 0 (u64::MAX).
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
                mode,
                args.direct_solc_evm,
                args.strict_template_errors,
                args.output_dir.as_deref(),
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
    let final_completed = completed.load(Ordering::Relaxed);
    let final_count = divergences.load(Ordering::Relaxed);
    match mode {
        Mode::Differential => {
            log::info!("done. divergences={final_count}");
            match (args.stop_on_divergence, final_count) {
                (true, n) if n > 0 => ExitCode::from(2),
                (_, 0) => ExitCode::SUCCESS,
                _ => ExitCode::from(1),
            }
        }
        Mode::Emit => {
            log::info!("done. emitted={final_completed} contracts");
            ExitCode::SUCCESS
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Compile + diff the case through both backends.
    Differential,
    /// Write the generated Solidity source to disk; no compile.
    Emit,
}

/// `(seed, iter)` pair → same case; a finding at any parallelism
/// reproduces at parallelism 1 with the same seed.
#[allow(clippy::too_many_arguments)]
fn run_iteration(
    iter: u64,
    seed: u64,
    input_size: usize,
    verbose: bool,
    stop_on_divergence: bool,
    mode: Mode,
    direct_solc_evm: bool,
    strict_template_errors: bool,
    output_dir: Option<&Path>,
    divergences: &AtomicU64,
    completed: &AtomicU64,
    stop: &AtomicBool,
    print_lock: &Mutex<()>,
) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed.wrapping_add(iter));
    let mut buffer = vec![0u8; input_size];
    rng.fill_bytes(&mut buffer);
    let mut u = Unstructured::new(&buffer);
    let case = match SolidityCase::arbitrary(&mut u) {
        Ok(case) => case,
        Err(error) => {
            log::debug!("iter {iter}: arbitrary exhausted: {error}");
            return;
        }
    };

    match mode {
        Mode::Emit => {
            let dir = output_dir.expect("emit mode without output_dir");
            let filename = format!("iter_{iter:06}_{}.sol", case.contract_name);
            let path = dir.join(filename);
            if let Err(error) = std::fs::write(&path, &case.source) {
                log::warn!("iter {iter}: write {} failed: {error}", path.display());
            } else if verbose {
                let _g = print_lock.lock().expect("print_lock poisoned");
                println!("wrote {}", path.display());
            }
        }
        Mode::Differential => {
            let result = if direct_solc_evm {
                run_case_solc_evm(&case)
            } else {
                run_case(&case)
            };
            // Compile failures on the EVM frontend (solc or revive-yul
            // roundtrip) signal a generator bug, not a backend bug.
            // Skip silently unless --strict-template-errors.
            if !strict_template_errors
                && matches!(
                    &result,
                    Err(Divergence::EvmCompile(_)) | Err(Divergence::YulRoundtripCompile(_))
                )
            {
                log::debug!("iter {iter}: template rejected by EVM frontend, skipping");
            } else {
                match result {
                    Ok(report) => {
                        if verbose {
                            let _g = print_lock.lock().expect("print_lock poisoned");
                            print_observations(iter, &case, &report);
                        } else {
                            log::debug!("iter {iter}: ok ({})", case.contract_name);
                        }
                    }
                    Err(divergence) => {
                        divergences.fetch_add(1, Ordering::Relaxed);
                        let _g = print_lock.lock().expect("print_lock poisoned");
                        print_divergence(iter, seed, &case, &divergence);
                        if stop_on_divergence {
                            stop.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }

    // The worker whose increment crosses a PROGRESS_DOT_EVERY
    // boundary prints — exactly one printer per dot.
    let prev = completed.fetch_add(1, Ordering::Relaxed);
    if (prev + 1) % PROGRESS_DOT_EVERY == 0 {
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, ".");
        let _ = stderr.flush();
    }
}

fn print_observations(iter: u64, case: &SolidityCase, report: &CompareReport) {
    println!("--- iter {iter} {} ---", case.contract_name);
    println!(
        "deploy_reverted: evm={}, pvm={}",
        report.evm.deploy_reverted, report.pvm.deploy_reverted
    );
    for (i, (a, b)) in report.evm.actions.iter().zip(report.pvm.actions.iter()).enumerate() {
        let arg = case
            .actions
            .get(i)
            .map(|a| hex::encode(a.argument))
            .unwrap_or_default();
        println!(
            "  action[{i}] arg=0x{arg}\n    evm: revert={} ret=0x{}\n    pvm: revert={} ret=0x{}",
            a.reverted,
            hex::encode(&a.return_data),
            b.reverted,
            hex::encode(&b.return_data)
        );
    }
}

fn print_divergence(iter: u64, seed: u64, case: &SolidityCase, div: &Divergence) {
    println!("---");
    println!("# revive-fuzz divergence");
    println!("seed: {seed}");
    println!("iteration: {iter}");
    println!("contract: {}", case.contract_name);
    println!("divergence: {div}");
    print_divergence_values(div);
    println!("source:");
    println!("{}", case.source);
    println!("constructor_args ({}):", case.constructor_args.len());
    for (i, arg) in case.constructor_args.iter().enumerate() {
        println!("  [{i}] 0x{}", hex::encode(arg));
    }
    println!("actions ({}):", case.actions.len());
    for (i, action) in case.actions.iter().enumerate() {
        println!("  [{i}] fn_0(0x{})", hex::encode(action.argument));
    }
    print_reproduce(case);
    println!("---");
}

/// Print shell snippets to recompile and run deploy + action[0] on
/// each backend. For multi-action sequences, rerun the fuzzer at
/// `--iterations 1 --seed <derived>` to drive the full sequence.
fn print_reproduce(case: &SolidityCase) {
    use revive_fuzz::observe::{action_calldata, constructor_calldata};

    let name = &case.contract_name;
    let constructor = hex::encode(constructor_calldata(case));
    let action0 = case.actions.first();

    println!("reproduce:");
    println!("  # save the `source` block above to /tmp/{name}.sol, then:");
    println!();
    println!("  # --- resolc → PVM (revive-runner) ---");
    println!("  resolc -O3 --bin /tmp/{name}.sol -o /tmp/{name}.pvm");
    if let Some(action) = action0 {
        let calldata = hex::encode(action_calldata(action));
        println!(
            "  revive-runner --file /tmp/{name}.pvm --deploy-calldata 0x{constructor} --calldata 0x{calldata}"
        );
    } else {
        println!("  revive-runner --file /tmp/{name}.pvm --deploy-calldata 0x{constructor}");
    }
    println!();
    println!("  # --- solc → EVM (geth `evm` tool) ---");
    println!("  solc --bin --optimize /tmp/{name}.sol > /tmp/{name}.evm.txt");
    println!(
        "  CODE=$(awk '/^[0-9a-fA-F]+$/ {{ print; exit }}' /tmp/{name}.evm.txt)"
    );
    println!("  evm --code \"$CODE\" --input 0x{constructor} run    # deploy");
    if let Some(action) = action0 {
        let calldata = hex::encode(action_calldata(action));
        println!(
            "  # action[0]: rerun `evm` against the deployed-runtime bytecode with --input 0x{calldata}"
        );
        println!("  # state continuation across actions requires a chained harness; rerun");
        println!("  # the fuzzer with `--iterations 1 --seed <seed_above>` for the full sequence.");
    }
}

/// Byte-level detail for a divergence — `Divergence`'s `Display`
/// carries only lengths and flags.
fn print_divergence_values(div: &Divergence) {
    match div {
        // Compile-failure variants already carry the payload in Display.
        Divergence::YulRoundtripCompile(_)
        | Divergence::EvmCompile(_)
        | Divergence::PvmCompile(_) => {}
        Divergence::DeployRevert { evm, pvm } => {
            println!("values:");
            println!("  evm deploy_reverted: {evm}");
            println!("  pvm deploy_reverted: {pvm}");
        }
        Divergence::ActionCount { evm, pvm } => {
            println!("values:");
            println!("  evm action count: {evm}");
            println!("  pvm action count: {pvm}");
        }
        Divergence::ActionRevert { index, evm, pvm } => {
            println!("values:");
            println!("  action[{index}] evm reverted: {evm}");
            println!("  action[{index}] pvm reverted: {pvm}");
        }
        Divergence::ActionReturnData { index, full, .. } => {
            let (a, b) = full.as_ref();
            println!("values:");
            println!("  action[{index}] evm return: 0x{}", hex::encode(a));
            println!("  action[{index}] pvm return: 0x{}", hex::encode(b));
        }
    }
}

