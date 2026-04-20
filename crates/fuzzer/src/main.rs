//! Differential fuzzer: compares EVM (geth `evm`) vs PVM (pallet-revive) execution.
//!
//! Architecture: a parent process discovers `.sol` files and spawns N worker
//! subprocesses in parallel. Each worker handles a single contract: compiles
//! it once for both targets, then runs every fuzz input against both and
//! reports divergences. Worker memory is capped via `RLIMIT_AS`, so an OOM
//! takes down the worker — not the host.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use alloy_primitives::{keccak256, U256};
use anyhow::{bail, Context, Result};
use clap::Parser;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

const WORKER_PROTOCOL_VERSION: u32 = 1;
/// Per-worker virtual-address cap. Must be large because LLVM/rayon reserve
/// huge VA regions even when their resident set is small.
const DEFAULT_MEMORY_LIMIT_MB: u64 = 16 * 1024;

#[derive(Parser)]
struct Cli {
    /// Directory containing `.sol` test files (or a single `.sol` file).
    #[arg(short, long)]
    corpus: PathBuf,

    /// Number of random inputs per function.
    #[arg(short, long, default_value_t = 64)]
    num_inputs: usize,

    /// Random seed.
    #[arg(short, long, default_value_t = 42)]
    seed: u64,

    /// Only run contracts whose path contains this substring.
    #[arg(short, long)]
    filter: Option<String>,

    /// Stop on first divergence.
    #[arg(long, default_value_t = false)]
    fail_fast: bool,

    /// Number of parallel worker processes (default: half of CPU count).
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Per-worker virtual-memory cap in MB (RLIMIT_AS).
    #[arg(long, default_value_t = DEFAULT_MEMORY_LIMIT_MB)]
    memory_limit_mb: u64,

    /// Per-input wall-clock timeout in seconds (kills worker if exceeded).
    #[arg(long, default_value_t = 120)]
    worker_timeout_secs: u64,

    /// Internal: run as a worker for the given .sol file (one path).
    #[arg(long, hide = true)]
    worker: Option<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    if let Some(path) = &cli.worker {
        return run_worker(path, &cli);
    }

    run_parent(cli)
}

// ---------------------------------------------------------------------------
// Parent: discovery + worker pool
// ---------------------------------------------------------------------------

fn run_parent(cli: Cli) -> Result<()> {
    let sol_files = discover_sol_files(&cli.corpus)?;
    if sol_files.is_empty() {
        bail!("no .sol files found in {}", cli.corpus.display());
    }

    let filtered: Vec<PathBuf> = sol_files
        .into_iter()
        .filter(|p| match &cli.filter {
            Some(s) => p.to_string_lossy().contains(s.as_str()),
            None => true,
        })
        .collect();

    let jobs = cli.jobs.unwrap_or_else(|| (num_cpus::get() / 2).max(1));

    eprintln!(
        "fuzzing {} files with {} workers, {} MB cap each",
        filtered.len(),
        jobs,
        cli.memory_limit_mb
    );

    let exe = std::env::current_exe().context("current_exe")?;
    let queue = Arc::new(crossbeam_queue());
    for p in &filtered {
        queue.push(p.clone());
    }

    let total_calls = Arc::new(AtomicU64::new(0));
    let total_divergences = Arc::new(AtomicU64::new(0));
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let start = Instant::now();

    let mut handles = Vec::with_capacity(jobs);
    for _ in 0..jobs {
        let exe = exe.clone();
        let queue = queue.clone();
        let total_calls = total_calls.clone();
        let total_divergences = total_divergences.clone();
        let stop_flag = stop_flag.clone();
        let memory_limit_mb = cli.memory_limit_mb;
        let timeout = cli.worker_timeout_secs;
        let num_inputs = cli.num_inputs;
        let seed = cli.seed;
        let fail_fast = cli.fail_fast;

        handles.push(std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                let Some(sol_path) = queue.pop() else { break };
                let result = spawn_worker(WorkerInvocation {
                    exe: &exe,
                    sol_path: &sol_path,
                    memory_limit_mb,
                    timeout_secs: timeout,
                    num_inputs,
                    seed,
                });

                match result {
                    Ok(report) => {
                        total_calls.fetch_add(report.calls, Ordering::Relaxed);
                        total_divergences
                            .fetch_add(report.divergences.len() as u64, Ordering::Relaxed);
                        for div in &report.divergences {
                            print_divergence(&sol_path, div);
                        }
                        if !report.divergences.is_empty() && fail_fast {
                            stop_flag.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(WorkerFailure::Killed { reason }) => {
                        eprintln!(
                            "[worker] {} killed: {} (skipping)",
                            sol_path.display(),
                            reason
                        );
                    }
                    Err(WorkerFailure::Other(e)) => {
                        eprintln!("[worker] {} error: {}", sol_path.display(), e);
                    }
                }
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    let elapsed = start.elapsed();
    let calls = total_calls.load(Ordering::Relaxed);
    let divs = total_divergences.load(Ordering::Relaxed);
    eprintln!("\n=== Summary ===");
    eprintln!("calls:        {calls}");
    eprintln!("divergences:  {divs}");
    eprintln!("time:         {:.1}s", elapsed.as_secs_f64());
    if elapsed.as_secs_f64() > 0.0 {
        eprintln!(
            "rate:         {:.1} calls/s",
            calls as f64 / elapsed.as_secs_f64()
        );
    }

    if divs > 0 {
        std::process::exit(1);
    }
    Ok(())
}

// Tiny lock-free-ish work queue. We don't pull in crossbeam just for this.
fn crossbeam_queue() -> WorkQueue {
    WorkQueue::default()
}

#[derive(Default)]
struct WorkQueue {
    inner: std::sync::Mutex<std::collections::VecDeque<PathBuf>>,
}

impl WorkQueue {
    fn push(&self, p: PathBuf) {
        self.inner.lock().unwrap().push_back(p);
    }
    fn pop(&self) -> Option<PathBuf> {
        self.inner.lock().unwrap().pop_front()
    }
}

struct WorkerInvocation<'a> {
    exe: &'a Path,
    sol_path: &'a Path,
    memory_limit_mb: u64,
    timeout_secs: u64,
    num_inputs: usize,
    seed: u64,
}

enum WorkerFailure {
    Killed { reason: String },
    Other(anyhow::Error),
}

enum WatchdogState {
    Running,
    ChildExited,
    FiredKill,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WorkerReport {
    version: u32,
    calls: u64,
    divergences: Vec<Divergence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Divergence {
    contract: String,
    function: String,
    calldata_hex: String,
    /// Panic message from the differential assertion in revive-runner.
    detail: String,
}

fn spawn_worker(inv: WorkerInvocation) -> Result<WorkerReport, WorkerFailure> {
    let mut cmd = Command::new(inv.exe);
    cmd.arg("--worker")
        .arg(inv.sol_path)
        .arg("--corpus")
        .arg(inv.sol_path) // unused in worker mode but required by clap
        .arg("--num-inputs")
        .arg(inv.num_inputs.to_string())
        .arg("--seed")
        .arg(inv.seed.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Pre-exec: cap address space so this worker cannot eat the host.
    let limit_bytes = inv.memory_limit_mb.saturating_mul(1024 * 1024);
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(move || {
            let rlim = libc::rlimit {
                rlim_cur: limit_bytes as libc::rlim_t,
                rlim_max: limit_bytes as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| WorkerFailure::Other(e.into()))?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        let mut reader = BufReader::new(stdout);
        let _ = reader.read_to_string(&mut buf);
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            eprintln!("  | {line}");
        }
    });

    // Enforce per-worker timeout via a cancellable watchdog thread. We wake
    // the watchdog with a condvar when the child exits so we don't block the
    // parent for the full timeout on every worker.
    let pid = child.id();
    let timeout = std::time::Duration::from_secs(inv.timeout_secs);
    let watchdog_state = Arc::new((
        std::sync::Mutex::new(WatchdogState::Running),
        std::sync::Condvar::new(),
    ));
    let killed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watchdog = {
        let state = watchdog_state.clone();
        let killed = killed.clone();
        std::thread::spawn(move || {
            let (lock, cvar) = &*state;
            let mut guard = lock.lock().unwrap();
            let result = cvar.wait_timeout(guard, timeout).unwrap();
            guard = result.0;
            if matches!(*guard, WatchdogState::Running) && result.1.timed_out() {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
                killed.store(true, Ordering::Relaxed);
                *guard = WatchdogState::FiredKill;
            }
        })
    };

    let status = child.wait().map_err(|e| WorkerFailure::Other(e.into()))?;
    // Tell watchdog we're done so it stops sleeping.
    {
        let (lock, cvar) = &*watchdog_state;
        let mut guard = lock.lock().unwrap();
        if matches!(*guard, WatchdogState::Running) {
            *guard = WatchdogState::ChildExited;
        }
        cvar.notify_all();
    }
    let _ = watchdog.join();
    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let _ = stderr_handle.join();

    if killed.load(Ordering::Relaxed) {
        return Err(WorkerFailure::Killed {
            reason: format!("timed out after {}s", inv.timeout_secs),
        });
    }
    if !status.success() {
        // Common cause on cap hit: SIGKILL by OOM killer (signal 9) or abort
        // because malloc returned NULL after RLIMIT_AS.
        let signal = status_signal(&status);
        return Err(WorkerFailure::Killed {
            reason: format!("exit={:?} signal={:?}", status.code(), signal),
        });
    }

    serde_json::from_str::<WorkerReport>(stdout_buf.trim())
        .with_context(|| format!("parse worker report (stdout was: {stdout_buf:?})"))
        .map_err(WorkerFailure::Other)
}

fn status_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

// Replacement for Read::read_to_string on a BufRead. We can't import the trait
// twice, so use a tiny helper.
trait ReadToString {
    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize>;
}
impl<R: std::io::Read> ReadToString for BufReader<R> {
    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        std::io::Read::read_to_string(self, buf)
    }
}

fn print_divergence(sol_path: &Path, div: &Divergence) {
    eprintln!(
        "DIVERGENCE {}::{} fn {}",
        sol_path.display(),
        div.contract,
        div.function
    );
    eprintln!("  calldata: 0x{}", div.calldata_hex);
    let first_line = div.detail.lines().next().unwrap_or("");
    eprintln!("  detail:   {}", first_line);
}

// ---------------------------------------------------------------------------
// Worker: compile once + run every input
// ---------------------------------------------------------------------------

fn run_worker(sol_path: &Path, cli: &Cli) -> Result<()> {
    use revive_runner::{
        Code, Specs, SpecsAction, TestAddress, ALICE, BOB, CHARLIE, DEPOSIT_LIMIT, GAS_LIMIT,
    };

    let source = std::fs::read_to_string(sol_path)
        .with_context(|| format!("read {}", sol_path.display()))?;

    let abi_contracts = solc_extract_abi(sol_path, &source)?;
    if abi_contracts.is_empty() {
        emit_report(WorkerReport::default_with_version());
        return Ok(());
    }
    let (contract_name, functions) = match abi_contracts
        .into_iter()
        .find(|(_, funcs)| !funcs.is_empty())
    {
        Some(x) => x,
        None => {
            emit_report(WorkerReport::default_with_version());
            return Ok(());
        }
    };

    // Pre-flight: try a no-arg deploy on EVM. If it reverts, the contract
    // wants constructor args we don't know how to synthesize, so skip rather
    // than producing a wall of bogus "deploy mismatch" divergences.
    let preflight = std::panic::catch_unwind(|| {
        revive_differential::Evm::default()
            .code_blob(
                hex::encode(resolc::test_utils::compile_evm_deploy_code(
                    &contract_name,
                    &source,
                    true,
                    Default::default(),
                ))
                .as_bytes()
                .to_vec(),
            )
            .deploy(true)
            .run()
    });
    let preflight_ok = match preflight {
        Ok(log) => log.account_deployed.is_some() && log.output.error.is_none(),
        Err(_) => false,
    };
    if !preflight_ok {
        eprintln!(
            "[{}] skipping {}: EVM deploy with empty calldata failed (needs ctor args?)",
            sol_path.display(),
            contract_name
        );
        emit_report(WorkerReport::default_with_version());
        return Ok(());
    }

    eprintln!(
        "[{}] fuzzing {} ({} functions)",
        sol_path.display(),
        contract_name,
        functions.len()
    );

    let mut rng = rand::rngs::SmallRng::seed_from_u64(cli.seed);
    let mut report = WorkerReport::default_with_version();

    for func in &functions {
        let inputs = generate_inputs(&func.input_types, cli.num_inputs, &mut rng);
        for args in &inputs {
            let calldata = encode_calldata(&func.selector, args);
            report.calls += 1;

            // Differential mode runs EVM (geth) + PVM (pallet-revive) and
            // asserts they agree on output, balance, and storage. resolc's
            // process-local cache means recompilation is free after the
            // first call.
            let result = std::panic::catch_unwind(|| {
                let specs = Specs {
                    differential: true,
                    balances: vec![
                        (ALICE, 1_000_000_000_000),
                        (BOB, 1_000_000_000_000),
                        (CHARLIE, 1_000_000_000_000),
                    ],
                    actions: vec![
                        SpecsAction::Instantiate {
                            origin: TestAddress::Alice,
                            value: 0,
                            gas_limit: Some(GAS_LIMIT),
                            storage_deposit_limit: Some(DEPOSIT_LIMIT),
                            code: Code::Solidity {
                                path: Some(sol_path.to_path_buf()),
                                solc_optimizer: Some(true),
                                contract: contract_name.clone(),
                                libraries: Default::default(),
                            },
                            data: vec![],
                            salt: Default::default(),
                        },
                        SpecsAction::Call {
                            origin: TestAddress::Alice,
                            dest: TestAddress::Instantiated(0),
                            value: 0,
                            gas_limit: Some(GAS_LIMIT),
                            storage_deposit_limit: Some(DEPOSIT_LIMIT),
                            data: calldata.clone(),
                        },
                    ],
                };
                specs.run()
            });

            if let Err(panic_info) = result {
                let detail = panic_message(&panic_info);
                report.divergences.push(Divergence {
                    contract: contract_name.clone(),
                    function: func.name.clone(),
                    calldata_hex: hex::encode(&calldata),
                    detail,
                });
                if report.divergences.len() >= MAX_DIVERGENCES_PER_WORKER {
                    eprintln!("[{}] reached divergence cap, stopping", sol_path.display());
                    break;
                }
            }
        }
    }

    emit_report(report);
    Ok(())
}

fn panic_message(panic_info: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic_info.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "unknown panic".to_string()
    }
}

const MAX_DIVERGENCES_PER_WORKER: usize = 50;

impl WorkerReport {
    fn default_with_version() -> Self {
        Self {
            version: WORKER_PROTOCOL_VERSION,
            calls: 0,
            divergences: vec![],
        }
    }
}

fn emit_report(report: WorkerReport) {
    // Worker contract: stdout receives one JSON line, the report.
    // Anything else we want to surface goes to stderr.
    let json = serde_json::to_string(&report).expect("serialize report");
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(json.as_bytes());
    let _ = out.write_all(b"\n");
}

// ---------------------------------------------------------------------------
// solc ABI extraction + supported-type filter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AbiFunction {
    name: String,
    selector: [u8; 4],
    /// Canonical ABI types of inputs (e.g. ["uint256", "address"]).
    input_types: Vec<SupportedType>,
}

#[derive(Debug, Clone, Copy)]
enum SupportedType {
    /// Any uint*/int*/bool/address/bytesN — all fit in a single 32-byte word.
    Word,
}

/// Parse all function signatures from solc's ABI for every contract in the
/// file. Returns `(contract_name, fuzzable_functions)` pairs in source order.
/// A function is "fuzzable" iff *all* of its inputs are simple word-sized
/// types and it is `external` or `public` and not `view`/`pure` — though we
/// still include view/pure since they help find divergences cheaply.
fn solc_extract_abi(path: &Path, source: &str) -> Result<Vec<(String, Vec<AbiFunction>)>> {
    // We pass solc the source on stdin via --standard-json so we don't have
    // to deal with import paths in test fixtures (most have none anyway).
    let standard_json = serde_json::json!({
        "language": "Solidity",
        "sources": {
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "contract.sol".to_string()): {
                "content": source
            }
        },
        "settings": {
            "outputSelection": { "*": { "*": ["abi"] } }
        }
    });

    let mut child = Command::new("solc")
        .arg("--standard-json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn solc")?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(standard_json.to_string().as_bytes())?;
    let output = child.wait_with_output().context("solc wait")?;
    if !output.status.success() {
        bail!("solc failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse solc json")?;
    let Some(contracts) = json.get("contracts").and_then(|c| c.as_object()) else {
        return Ok(vec![]);
    };

    let mut result = Vec::new();
    for (_file, file_contracts) in contracts {
        let Some(map) = file_contracts.as_object() else {
            continue;
        };
        for (name, contract) in map {
            let Some(abi) = contract.get("abi").and_then(|a| a.as_array()) else {
                continue;
            };
            let funcs = parse_fuzzable_functions(abi);
            result.push((name.clone(), funcs));
        }
    }
    Ok(result)
}

fn parse_fuzzable_functions(abi: &[serde_json::Value]) -> Vec<AbiFunction> {
    let mut out = Vec::new();
    for entry in abi {
        if entry.get("type").and_then(|t| t.as_str()) != Some("function") {
            continue;
        }
        let Some(name) = entry.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let inputs = entry
            .get("inputs")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();

        // Build canonical signature and classify each input.
        let mut canonical = Vec::with_capacity(inputs.len());
        let mut classified = Vec::with_capacity(inputs.len());
        let mut supported = true;
        for input in &inputs {
            let ty = input.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if !is_simple_word_type(ty) {
                supported = false;
                break;
            }
            canonical.push(ty.to_string());
            classified.push(SupportedType::Word);
        }
        if !supported {
            continue;
        }

        let sig = format!("{}({})", name, canonical.join(","));
        let hash = keccak256(sig.as_bytes());
        let mut selector = [0u8; 4];
        selector.copy_from_slice(&hash[..4]);

        out.push(AbiFunction {
            name: name.to_string(),
            selector,
            input_types: classified,
        });
    }
    out
}

fn is_simple_word_type(ty: &str) -> bool {
    if ty == "bool" || ty == "address" {
        return true;
    }
    if let Some(rest) = ty.strip_prefix("uint") {
        return rest.is_empty() || rest.parse::<u32>().is_ok();
    }
    if let Some(rest) = ty.strip_prefix("int") {
        return rest.is_empty() || rest.parse::<u32>().is_ok();
    }
    if let Some(rest) = ty.strip_prefix("bytes") {
        // bytes (dynamic) excluded; only bytesN for 1<=N<=32.
        return rest
            .parse::<u32>()
            .map(|n| (1..=32).contains(&n))
            .unwrap_or(false);
    }
    false
}

// ---------------------------------------------------------------------------
// Input generation + calldata encoding
// ---------------------------------------------------------------------------

fn generate_inputs(
    types: &[SupportedType],
    num_random: usize,
    rng: &mut impl Rng,
) -> Vec<Vec<U256>> {
    if types.is_empty() {
        return vec![vec![]];
    }
    let boundary: [U256; 11] = [
        U256::ZERO,
        U256::from(1u64),
        U256::from(2u64),
        U256::from(31u64),
        U256::from(32u64),
        U256::from(0xFFu64),
        U256::from(0xFFFF_FFFFu64),
        U256::from(0xFFFF_FFFF_FFFF_FFFFu64),
        U256::from(1u64) << 128,
        U256::from(1u64) << 255,
        U256::MAX,
    ];

    let mut inputs = Vec::with_capacity(boundary.len() + num_random);
    // Boundary sweep: same boundary value across every parameter.
    for v in &boundary {
        inputs.push(vec![*v; types.len()]);
    }
    // Random tuples.
    for _ in 0..num_random {
        let mut tuple = Vec::with_capacity(types.len());
        for _ in 0..types.len() {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            tuple.push(U256::from_be_bytes(bytes));
        }
        inputs.push(tuple);
    }
    inputs
}

fn encode_calldata(selector: &[u8; 4], args: &[U256]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + args.len() * 32);
    data.extend_from_slice(selector);
    for arg in args {
        data.extend_from_slice(&arg.to_be_bytes::<32>());
    }
    data
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

fn discover_sol_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_file() {
        if root.extension().is_some_and(|e| e == "sol") {
            out.push(root.to_path_buf());
        }
        return Ok(out);
    }
    walk(root, &mut out)?;
    out.retain(|p| {
        // Skip eravm-only fixtures.
        p.file_stem()
            .map(|s| !s.to_string_lossy().ends_with("_eravm"))
            .unwrap_or(true)
    });
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, acc)?;
        } else if path.extension().is_some_and(|e| e == "sol") {
            acc.push(path);
        }
    }
    Ok(())
}
