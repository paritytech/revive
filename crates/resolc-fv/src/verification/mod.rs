//! Stating the chain's obligations and discharging them.
//!
//! One obligation per step, each handed to the checker for the pair of formats
//! it crosses. The checkers live one to a module below this one.

pub mod newyork;
pub mod newyork_llvm;
pub mod simulation;
pub mod smt;
pub mod yul_newyork;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use crate::common::Chain;
use crate::common::Content;
use crate::common::Format;
use crate::common::Obligation;
use crate::common::Proof;
use crate::common::ProofState;
use crate::common::Run;
use crate::common::Stage;
use crate::common::Verdict;
use crate::common::CODEGEN_STEP;
use crate::common::LLVM_DUMP_SUFFIX;
use crate::common::MISSING_STEP;
use crate::output::Overview;
use crate::serialize::write_log;
use crate::verification::smt::Solver;

/// States every obligation in `chains`, in pipeline order within each chain.
///
/// A pass's obligation is against the previous stage **of the same object**: a
/// contract's deploy and runtime halves run the same pass sequence, and pairing
/// across that boundary would compare unrelated code. The Yul translation is
/// stated against the Yul AST, the input it actually consumed, and codegen
/// closes the chain against the LLVM dump.
///
/// A step whose other side is missing — a subobject's first pass, whose input
/// sits inside the parent object's tree, or a chain with no dump to compare
/// against — is still planned, carrying the reason it cannot be stated.
pub fn plan_obligations(chains: &[Chain]) -> Vec<Obligation<'_>> {
    let mut obligations = Vec::new();

    for (index, chain) in chains.iter().enumerate() {
        let mut previous_by_object: BTreeMap<&str, &Stage> = BTreeMap::new();

        for stage in chain.stages.iter() {
            let object: &str = &stage.content.object;
            let content_to = Content::NewYork(&stage.content.ir);

            let content_from = match previous_by_object.get(object) {
                Some(previous) => Some(Content::NewYork(&previous.content.ir)),
                None => chain
                    .yul_ast
                    .as_ref()
                    .filter(|_| stage.from == Format::Yul)
                    .map(Content::Yul),
            };

            let source = match previous_by_object.get(object) {
                Some(previous) => previous.content.stage_id.to_string(),
                None => match (stage.from, chain.manifest.yul_ast.as_ref()) {
                    (Format::Yul, Some(yul_ast)) => yul_ast.clone(),
                    _ => MISSING_STEP.to_owned(),
                },
            };

            obligations.push(Obligation {
                chain: index,
                label: stage.content.stage_id.to_string(),
                source,
                target: stage.content.stage_id.to_string(),
                from: stage.from,
                to: stage.to,
                sides: match content_from {
                    Some(content_from) => Ok((content_from, content_to)),
                    None => Err(format!(
                        "nothing precedes `{}` in object `{object}` to compare it against",
                        stage.content.stage_id,
                    )),
                },
            });

            previous_by_object.insert(object, stage);
        }

        // Codegen closes the chain. The compiler guarantees the last stage holds
        // the IR it lowered, so that stage is the newyork side of the obligation.
        if let Some(last) = chain.stages.last() {
            obligations.push(Obligation {
                chain: index,
                label: CODEGEN_STEP.to_owned(),
                source: last.content.stage_id.to_string(),
                target: chain
                    .llvm_ir_name
                    .clone()
                    .unwrap_or_else(|| MISSING_STEP.to_owned()),
                from: Format::NewYork,
                to: Format::Llvm,
                sides: match chain.llvm_ir.as_deref() {
                    Some(llvm_ir) => Ok((
                        Content::NewYork(&last.content.ir),
                        Content::Llvm(llvm_ir),
                    )),
                    None => Err(format!(
                        "no `{LLVM_DUMP_SUFFIX}` LLVM dump beside the manifest to compare the final IR against"
                    )),
                },
            });
        }
    }

    obligations
}

/// Runs every obligation, at most `jobs` at a time, updating the overview live.
///
/// Obligations are independent, so the provers share nothing but a cursor into
/// the work list. Results are keyed by obligation index, which keeps the report
/// in pipeline order however the provers interleave.
pub fn prove(
    obligations: &[Obligation<'_>],
    chains: &[Chain],
    jobs: usize,
    log_directory: Option<&Path>,
    solver: &Solver,
) -> Run {
    let states: Vec<AtomicU8> = obligations
        .iter()
        .map(|_| AtomicU8::new(ProofState::Pending.as_u8()))
        .collect();
    let verdicts: Mutex<BTreeMap<usize, Verdict>> = Mutex::new(BTreeMap::new());
    let log_failures: Mutex<Vec<String>> = Mutex::new(Vec::new());

    let overview = Overview::new(obligations, chains);
    overview.refresh(&states);

    let cursor = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let index = cursor.fetch_add(1, Ordering::Relaxed);
                let Some(obligation) = obligations.get(index) else {
                    break;
                };

                states[index].store(ProofState::Running.as_u8(), Ordering::Relaxed);
                overview.refresh(&states);

                let proof = discharge(obligation, solver);

                // Written from the prover itself: each log is its own file, so
                // there is nothing to serialize between threads, and a verdict
                // never outlives the record of how it was reached.
                if let Some(directory) = log_directory {
                    if let Err(error) =
                        write_log(directory, &chains[obligation.chain], obligation, &proof)
                    {
                        log_failures
                            .lock()
                            .expect("ICE: a prover panicked holding the log lock")
                            .push(error.to_string());
                    }
                }

                states[index].store(proof.verdict.state().as_u8(), Ordering::Relaxed);
                overview.refresh(&states);

                verdicts
                    .lock()
                    .expect("ICE: a prover panicked holding the verdict lock")
                    .insert(index, proof.verdict);
            });
        }
    });

    overview.finish();

    Run {
        verdicts: verdicts
            .into_inner()
            .expect("ICE: a prover panicked holding the verdict lock"),
        log_failures: log_failures
            .into_inner()
            .expect("ICE: a prover panicked holding the log lock"),
    }
}

/// Runs one obligation.
fn discharge(obligation: &Obligation<'_>, solver: &Solver) -> Proof {
    match obligation.sides.as_ref() {
        Ok((content_from, content_to)) => is_equivalent(
            obligation.from,
            obligation.to,
            content_from,
            content_to,
            solver,
        ),
        Err(reason) => Proof::unproven(reason.clone()),
    }
}

/// Decides whether `content_to` preserves the meaning of `content_from`.
///
/// Dispatches on the pair of formats, because the obligation between a Yul AST
/// and newyork IR is a different statement from the one between two newyork IRs
/// and needs its own checker.
fn is_equivalent(
    from: Format,
    to: Format,
    content_from: &Content<'_>,
    content_to: &Content<'_>,
    solver: &Solver,
) -> Proof {
    match (from, to, content_from, content_to) {
        (Format::Yul, Format::NewYork, Content::Yul(yul), Content::NewYork(newyork)) => {
            yul_newyork::is_equivalent(yul, newyork, solver)
        }
        (Format::NewYork, Format::NewYork, Content::NewYork(before), Content::NewYork(after)) => {
            newyork::is_equivalent(before, after, solver)
        }
        (Format::NewYork, Format::Llvm, Content::NewYork(newyork), Content::Llvm(llvm)) => {
            newyork_llvm::is_equivalent(newyork, llvm, solver)
        }
        _ => Proof::unproven(format!(
            "no checker covers a {from} input followed by a {to} output"
        )),
    }
}
