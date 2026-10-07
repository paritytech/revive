//! Talking to an SMT solver.
//!
//! The checks that will need a solver are not written yet, so what lives here is
//! the wiring and a probe: a fixed query, carrying no contract data, run once per
//! obligation that reaches a checker, whose answer is recorded at the head of the
//! proof's log.
//!
//! The solver is a binary this tool runs, installed separately, the way `solc`
//! is. Linking one in was tried and dropped: `smtlib`'s in-process Z3 backend
//! never releases its context, so a chain of any length exhausts memory.

use smtlib::backend::cvc5_binary::Cvc5Binary;
use smtlib::backend::z3_binary::Z3Binary;
use smtlib::prelude::*;
use smtlib::Backend;
use smtlib::Int;
use smtlib::SatResultWithModel;
use smtlib::Storage;

/// The two operands of the probe query, which has exactly one solution.
const PROBE_DIFFERENCE: i64 = 25;
/// See [`PROBE_DIFFERENCE`].
const PROBE_TOTAL: i64 = 204;

/// What the probe reports for a variable the model did not pin down.
const UNEVALUATED: &str = "?";

/// Which SMT solver the checks talk to.
///
/// Each is a binary, run as a child process. One has to be installed: there is
/// no solver built into this tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum SmtBackend {
    /// The `z3` binary.
    #[default]
    Z3,
    /// The `cvc5` binary.
    Cvc5,
}

impl SmtBackend {
    /// The binary this backend runs when `--smt-solver` does not say otherwise.
    pub fn default_command(self) -> &'static str {
        match self {
            Self::Z3 => "z3",
            Self::Cvc5 => "cvc5",
        }
    }
}

impl std::fmt::Display for SmtBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.pad(self.default_command())
    }
}

/// The solver a run talks to, with its binary already located.
#[derive(Clone, Debug)]
pub struct Solver {
    /// Which solver was asked for.
    backend: SmtBackend,
    /// The binary to run.
    executable: String,
}

impl Solver {
    /// Locates the solver for `backend`, honouring an `--smt-solver` override.
    ///
    /// The binary is looked up here rather than when the first obligation
    /// reaches it, so that a solver which is not installed is one message before
    /// any work starts rather than one per obligation afterwards.
    pub fn new(backend: SmtBackend, executable: Option<String>) -> anyhow::Result<Self> {
        let executable = executable.unwrap_or_else(|| backend.default_command().to_owned());
        if let Err(error) = which::which(executable.as_str()) {
            anyhow::bail!("The `{executable}` executable not found in ${{PATH}}: {error}");
        }

        Ok(Self {
            backend,
            executable,
        })
    }
}

impl std::fmt::Display for Solver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.pad(&self.executable)
    }
}

/// Exercises `solver`, as a line for a proof's transcript.
///
/// A solver that answers wrongly or not at all is reported rather than raised:
/// it was there when the run started, and the place to read about it going quiet
/// is the log beside the obligation it would have discharged.
pub fn probe(solver: &Solver) -> String {
    match run_probe(solver) {
        Ok(answer) => format!("smt: {solver} answered {answer}\n"),
        Err(error) => format!("smt: {solver} did not answer: {error}\n"),
    }
}

/// Runs the probe query on `solver`.
fn run_probe(solver: &Solver) -> anyhow::Result<String> {
    let storage = Storage::new();

    match solver.backend {
        SmtBackend::Z3 => solve_probe(&storage, Z3Binary::new(&solver.executable)?),
        SmtBackend::Cvc5 => solve_probe(&storage, Cvc5Binary::new(&solver.executable)?),
    }
}

/// Asks the solver for the one pair of integers that satisfies the probe.
///
/// Deliberately a query whose answer is known, so that a wrong answer is as
/// visible in the log as no answer at all.
fn solve_probe<B: Backend>(storage: &Storage, backend: B) -> anyhow::Result<String> {
    let mut solver = smtlib::Solver::new(storage, backend)?;

    let x = Int::new_const(storage, "x");
    let y = Int::new_const(storage, "y");
    solver.assert(x._eq(y + PROBE_DIFFERENCE))?;
    solver.assert(x._eq(PROBE_TOTAL))?;

    let answer = match solver.check_sat_with_model()? {
        SatResultWithModel::Sat(model) => format!(
            "sat (x = {}, y = {})",
            model
                .eval(x)
                .map(|value| value.to_string())
                .unwrap_or_else(|| UNEVALUATED.to_owned()),
            model
                .eval(y)
                .map(|value| value.to_string())
                .unwrap_or_else(|| UNEVALUATED.to_owned()),
        ),
        SatResultWithModel::Unsat => "unsat".to_owned(),
        SatResultWithModel::Unknown => "unknown".to_owned(),
    };

    Ok(answer)
}
