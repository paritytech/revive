//! Formal verification tooling for the newyork IR.
//!
//! `resolc` emits one artifact per optimization pass under `--emit-ir-stages`,
//! indexed by a manifest. This binary consumes that chain and discharges one
//! equivalence obligation per adjacent pair of stages, so that the optimizer's
//! output can be checked against its input without trusting the optimizer.
//!
//! The schema of both the manifest and the stage artifacts lives in
//! `revive_newyork::export`, shared with the compiler that writes them.
//!
//! This is a scaffold: the chain is parsed into [`common::Stage`] values,
//! planned into [`common::Obligation`]s and run `--jobs` at a time behind a live
//! overview. The checks themselves are stubs that sleep and decide at random.

mod common;
mod output;
mod parse;
mod serialize;
mod verification;

use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use revive_common::EXIT_CODE_FAILURE;
use revive_common::EXTENSION_NEWYORK_MANIFEST;

use crate::verification::smt::SmtBackend;
use crate::verification::smt::Solver;

/// Reads the IR stage artifacts written by `resolc --emit-ir-stages` and checks
/// the optimizer's pass chain against them.
/// Example: resolc-fv --stages './build/' --jobs 8
#[derive(Debug, Parser)]
#[command(name = "The newyork IR verifier", arg_required_else_help = true)]
pub struct Arguments {
    /// The directory holding the stage artifacts and their manifest, as passed
    /// to `resolc --debug-output-dir`.
    #[arg(long = "stages")]
    pub stages: PathBuf,

    /// How many proofs to run at once. Defaults to the available parallelism.
    #[arg(short = 'j', long = "jobs", alias = "j")]
    pub jobs: Option<usize>,

    /// Where to write one log per proof.
    /// Without it the proofs are run but their logs are discarded.
    #[arg(long = "log-dir")]
    pub log_dir: Option<PathBuf>,

    /// Which SMT solver the checks talk to. It has to be installed separately.
    #[arg(long = "smt-backend", value_enum, default_value_t = SmtBackend::Z3)]
    pub smt_backend: SmtBackend,

    /// The solver binary to run.
    /// Defaults to the backend's usual name, looked up on `PATH`.
    #[arg(long = "smt-solver")]
    pub smt_solver: Option<String>,
}

impl Arguments {
    /// How many proofs may run at once.
    ///
    /// Obligations are independent of one another — each compares two artifacts
    /// that are already on disk — so the only bound is how many provers the
    /// machine should be running.
    pub fn job_count(&self) -> anyhow::Result<usize> {
        let jobs = match self.jobs {
            Some(jobs) => jobs,
            None => std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1),
        };
        anyhow::ensure!(jobs > 0, "`--jobs` must be at least 1");

        Ok(jobs)
    }

    /// Where the proof logs go, or `None` to discard them.
    ///
    /// There is no default: writing logs means writing into a directory the
    /// caller did not nominate, and the artifact directory belongs to the
    /// compiler. Discarding is the quieter wrong answer of the two, so it is
    /// the one that happens by default — loudly.
    pub fn log_directory(&self) -> Option<&Path> {
        self.log_dir.as_deref()
    }
}

fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let jobs = arguments.job_count()?;
    let solver = Solver::new(arguments.smt_backend, arguments.smt_solver.clone())?;

    let manifests = parse::manifest_paths(&arguments.stages)?;
    anyhow::ensure!(
        !manifests.is_empty(),
        "no `*.{}` manifest in {}; was the contract compiled with `resolc --emit-ir-stages`?",
        EXTENSION_NEWYORK_MANIFEST,
        arguments.stages.display(),
    );

    let mut chains = Vec::with_capacity(manifests.len());
    for manifest_path in manifests {
        chains.push(parse::parse_chain(manifest_path)?);
    }
    for chain in chains.iter() {
        output::print_chain(chain);
    }

    let obligations = verification::plan_obligations(&chains);

    let log_directory = arguments.log_directory();
    if let Some(directory) = log_directory {
        std::fs::create_dir_all(directory).map_err(|error| {
            anyhow::anyhow!(
                "creating the log directory {}: {error}",
                directory.display()
            )
        })?;
    }

    println!();
    output::print_legend(jobs, obligations.len(), log_directory);

    let run = verification::prove(&obligations, &chains, jobs, log_directory, &solver);
    let totals = output::report(&obligations, &run, log_directory);

    if totals.failed > 0 {
        std::process::exit(EXIT_CODE_FAILURE);
    }

    Ok(())
}
