use std::path::{Path, PathBuf};
use std::time::Instant;

use alloy_primitives::U256;
use anyhow::{bail, Context, Result};
use clap::Parser;
use rand::{Rng, SeedableRng};
use regex::Regex;
use revive_runner::{
    Code, Specs, SpecsAction, TestAddress, ALICE, BOB, CHARLIE, DEPOSIT_LIMIT, GAS_LIMIT,
};

/// Differential fuzzer for revive: compares EVM (geth) vs PVM (pallet-revive).
#[derive(Parser)]
struct Cli {
    /// Directory containing .sol test files
    #[arg(short, long)]
    corpus: PathBuf,

    /// Number of random inputs to generate per function
    #[arg(short, long, default_value_t = 200)]
    num_inputs: usize,

    /// Random seed
    #[arg(short, long, default_value_t = 42)]
    seed: u64,

    /// Only run contracts matching this substring
    #[arg(short, long)]
    filter: Option<String>,

    /// Stop on first divergence
    #[arg(long, default_value_t = false)]
    fail_fast: bool,

    /// Batch size: number of calls per Specs run
    #[arg(short, long, default_value_t = 50)]
    batch_size: usize,
}

/// A detected function signature in a Solidity contract.
#[derive(Debug, Clone)]
struct FunctionSig {
    name: String,
    param_count: usize,
    selector: [u8; 4],
}

/// A divergence between EVM and PVM.
#[derive(Debug)]
struct Divergence {
    contract_file: String,
    contract_name: String,
    function: String,
    calldata_hex: String,
    detail: String,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let sol_files = discover_sol_files(&cli.corpus)?;
    if sol_files.is_empty() {
        bail!("No .sol files found in {}", cli.corpus.display());
    }
    eprintln!(
        "Found {} .sol files in {}",
        sol_files.len(),
        cli.corpus.display()
    );

    let mut rng = rand::rngs::SmallRng::seed_from_u64(cli.seed);
    let mut total_calls = 0u64;
    let mut total_divergences = 0u64;
    let mut all_divergences: Vec<Divergence> = Vec::new();
    let start = Instant::now();

    for sol_path in &sol_files {
        let short_name = sol_path
            .strip_prefix(&cli.corpus)
            .unwrap_or(sol_path)
            .display()
            .to_string();

        if let Some(ref filter) = cli.filter {
            if !short_name.contains(filter.as_str()) {
                continue;
            }
        }

        let source = match std::fs::read_to_string(sol_path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("skip {short_name}: {e}");
                continue;
            }
        };

        // Strip inline test JSON (//! lines) to get pure Solidity
        let clean_source = strip_inline_json(&source);

        let contract_name = match extract_contract_name(&clean_source) {
            Some(n) => n,
            None => {
                log::debug!("skip {short_name}: no contract found");
                continue;
            }
        };

        let functions = extract_functions(&clean_source);
        if functions.is_empty() {
            log::debug!("skip {short_name}: no external functions");
            continue;
        }

        eprint!("{short_name} ({contract_name}): ");

        // Generate all calldata for all functions
        let mut all_calldata: Vec<(String, Vec<u8>)> = Vec::new();
        for func in &functions {
            let inputs = generate_inputs(func.param_count, cli.num_inputs, &mut rng);
            for args in &inputs {
                let calldata = encode_calldata(&func.selector, args);
                all_calldata.push((func.name.clone(), calldata));
            }
        }

        eprint!("{} calls... ", all_calldata.len());

        // Process in batches
        let mut contract_divergences = 0u64;
        for batch in all_calldata.chunks(cli.batch_size) {
            let mut actions: Vec<SpecsAction> = Vec::new();

            // Deploy
            actions.push(SpecsAction::Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: Some(DEPOSIT_LIMIT),
                code: Code::Solidity {
                    path: Some(sol_path.clone()),
                    solc_optimizer: Some(false),
                    contract: contract_name.clone(),
                    libraries: Default::default(),
                },
                data: vec![],
                salt: Default::default(),
            });

            // Add all calls in this batch
            for (_func_name, calldata) in batch {
                actions.push(SpecsAction::Call {
                    origin: TestAddress::Alice,
                    dest: TestAddress::Instantiated(0),
                    value: 0,
                    gas_limit: Some(GAS_LIMIT),
                    storage_deposit_limit: Some(DEPOSIT_LIMIT),
                    data: calldata.clone(),
                });
            }

            let specs = Specs {
                differential: true,
                balances: vec![
                    (ALICE, 1_000_000_000_000),
                    (BOB, 1_000_000_000_000),
                    (CHARLIE, 1_000_000_000_000),
                ],
                actions,
            };

            // Run and catch any assertion failures (divergences)
            let result = std::panic::catch_unwind(move || {
                specs.run();
            });

            total_calls += batch.len() as u64;

            if let Err(panic_info) = result {
                // A divergence was found! The batch failed.
                let detail = if let Some(msg) = panic_info.downcast_ref::<String>() {
                    msg.clone()
                } else if let Some(msg) = panic_info.downcast_ref::<&str>() {
                    msg.to_string()
                } else {
                    "unknown assertion failure".to_string()
                };

                contract_divergences += 1;
                total_divergences += 1;

                // We don't know exactly which call failed, but we can narrow down
                let func_name = &batch[0].0;
                let calldata_hex = hex::encode(&batch[0].1);

                let div = Divergence {
                    contract_file: short_name.clone(),
                    contract_name: contract_name.clone(),
                    function: func_name.clone(),
                    calldata_hex,
                    detail: detail.clone(),
                };

                eprintln!();
                print_divergence(&div);
                all_divergences.push(div);

                if cli.fail_fast {
                    print_summary(total_calls, total_divergences, start.elapsed());
                    std::process::exit(1);
                }

                // Try to narrow down: re-run batch individually
                for (func_name, calldata) in batch {
                    let single_specs = Specs {
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
                                    path: Some(sol_path.clone()),
                                    solc_optimizer: Some(false),
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

                    let single_result = std::panic::catch_unwind(move || {
                        single_specs.run();
                    });

                    if let Err(panic_info) = single_result {
                        let detail = if let Some(msg) = panic_info.downcast_ref::<String>() {
                            msg.clone()
                        } else if let Some(msg) = panic_info.downcast_ref::<&str>() {
                            msg.to_string()
                        } else {
                            "unknown".to_string()
                        };

                        eprintln!(
                            "  ISOLATED: {}::{} fn {} calldata=0x{}",
                            short_name,
                            contract_name,
                            func_name,
                            hex::encode(calldata),
                        );
                        eprintln!("    {detail}");
                    }
                }
            }
        }

        if contract_divergences == 0 {
            eprintln!("OK");
        } else {
            eprintln!("{contract_divergences} batch(es) with divergences");
        }
    }

    print_summary(total_calls, total_divergences, start.elapsed());

    if !all_divergences.is_empty() {
        eprintln!("\n=== All divergences ===");
        for div in &all_divergences {
            print_divergence(div);
        }
        std::process::exit(1);
    }

    Ok(())
}

/// Strip //! lines (inline test JSON) from source to get pure Solidity.
fn strip_inline_json(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Discover .sol files recursively, skipping _eravm variants.
fn discover_sol_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_file() && dir.extension().is_some_and(|e| e == "sol") {
        files.push(dir.to_path_buf());
        return Ok(files);
    }
    for path in walkdir(dir)? {
        if path.extension().is_some_and(|e| e == "sol") {
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            // Skip eravm-specific tests
            if name.ends_with("_eravm") {
                continue;
            }
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn walkdir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    if !dir.is_dir() {
        return Ok(result);
    }
    for entry in std::fs::read_dir(dir).context("read_dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            result.extend(walkdir(&path)?);
        } else {
            result.push(path);
        }
    }
    Ok(result)
}

/// Extract the first contract name from Solidity source.
fn extract_contract_name(source: &str) -> Option<String> {
    let re = Regex::new(r"contract\s+(\w+)").unwrap();
    re.captures(source).map(|c| c[1].to_string())
}

/// Extract external/public function signatures from Solidity source.
fn extract_functions(source: &str) -> Vec<FunctionSig> {
    let re = Regex::new(r"function\s+(\w+)\s*\(([^)]*)\)\s+(?:external|public)").unwrap();

    let mut funcs = Vec::new();
    for cap in re.captures_iter(source) {
        let name = cap[1].to_string();
        let params_str = cap[2].trim();
        let param_count = if params_str.is_empty() {
            0
        } else {
            params_str.split(',').count()
        };

        // Extract just the type (first token per param) for selector computation
        let param_types: Vec<&str> = if params_str.is_empty() {
            vec![]
        } else {
            params_str
                .split(',')
                .map(|p| {
                    let p = p.trim();
                    p.split_whitespace().next().unwrap_or("uint256")
                })
                .collect()
        };

        let sig = format!("{}({})", name, param_types.join(","));
        let hash = alloy_primitives::keccak256(sig.as_bytes());
        let selector: [u8; 4] = hash[..4].try_into().unwrap();

        funcs.push(FunctionSig {
            name,
            param_count,
            selector,
        });
    }

    funcs
}

/// Generate fuzz inputs: boundary values + random uint256 tuples.
fn generate_inputs(param_count: usize, num_random: usize, rng: &mut impl Rng) -> Vec<Vec<U256>> {
    if param_count == 0 {
        return vec![vec![]];
    }

    let boundary: Vec<U256> = vec![
        U256::ZERO,
        U256::from(1u64),
        U256::from(2u64),
        U256::from(31u64),
        U256::from(32u64),
        U256::from(33u64),
        U256::from(0xFFu64),
        U256::from(0x100u64),
        U256::from(0xFFFFu64),
        U256::from(0x10000u64),
        U256::from(0xFFFF_FFFFu64),
        U256::from(1u64) << 32,
        U256::from(0xFFFF_FFFF_FFFF_FFFFu64),
        U256::from(1u64) << 64,
        U256::from(1u64) << 128,
        U256::from(1u64) << 255,
        (U256::from(1u64) << 255) - U256::from(1u64),
        U256::MAX,
        U256::MAX - U256::from(1u64),
    ];

    let mut inputs = Vec::new();

    if param_count == 1 {
        for v in &boundary {
            inputs.push(vec![*v]);
        }
    } else if param_count == 2 {
        for a in &boundary {
            for b in &boundary {
                inputs.push(vec![*a, *b]);
            }
        }
    } else {
        for _ in 0..boundary.len() * 3 {
            let mut tuple = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                let idx = rng.random_range(0..boundary.len());
                tuple.push(boundary[idx]);
            }
            inputs.push(tuple);
        }
    }

    // Random inputs
    for _ in 0..num_random {
        let mut tuple = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            tuple.push(U256::from_be_bytes(bytes));
        }
        inputs.push(tuple);
    }

    inputs
}

/// ABI-encode calldata: 4-byte selector + 32-byte padded args.
fn encode_calldata(selector: &[u8; 4], args: &[U256]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + args.len() * 32);
    data.extend_from_slice(selector);
    for arg in args {
        data.extend_from_slice(&arg.to_be_bytes::<32>());
    }
    data
}

fn print_divergence(div: &Divergence) {
    eprintln!(
        "  DIVERGENCE: {}::{} fn {}",
        div.contract_file, div.contract_name, div.function
    );
    eprintln!("    calldata: 0x{}", div.calldata_hex);
    eprintln!("    {}", div.detail);
}

fn print_summary(calls: u64, divergences: u64, elapsed: std::time::Duration) {
    eprintln!("\n=== Summary ===");
    eprintln!("Total calls:      {calls}");
    eprintln!("Divergences:      {divergences}");
    eprintln!("Time:             {:.1}s", elapsed.as_secs_f64());
    if elapsed.as_secs_f64() > 0.0 {
        eprintln!(
            "Rate:             {:.0} calls/sec",
            calls as f64 / elapsed.as_secs_f64()
        );
    }
}
