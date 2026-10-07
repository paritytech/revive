//! Everything the run puts on the terminal.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use console::Style;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;

use crate::common::Chain;
use crate::common::Format;
use crate::common::Obligation;
use crate::common::ProofState;
use crate::common::Run;
use crate::common::Totals;
use crate::common::Verdict;
use crate::common::CODEGEN_STEP;
use crate::common::MISSING_STEP;

/// What a run says when it has nowhere to put its proof logs.
pub const LOGS_DISCARDED_WARNING: &str = "warning: no `--log-dir` given, so each proof's log is \
     discarded as soon as its verdict is counted; pass `--log-dir <dir>` to keep them";

/// The live overview of the proof invocations.
///
/// One line per chain, each carrying a symbol per obligation in pipeline order,
/// so a symbol's position is the position of the step it stands for.
pub struct Overview {
    /// One bar per chain, in the order the chains were parsed.
    bars: Vec<ProgressBar>,
    /// The obligations each bar covers, as a range into the obligation list.
    ranges: Vec<Range<usize>>,
    /// Held so the bars keep their place on the terminal for the whole run.
    _multi: MultiProgress,
}

impl Overview {
    /// Lays out one line per chain.
    pub fn new(obligations: &[Obligation<'_>], chains: &[Chain]) -> Self {
        // The prefix column is sized from the longest name rather than fixed,
        // so that a deep source path does not push the symbols off the line.
        let width = chains
            .iter()
            .map(|chain| chain.label().len())
            .max()
            .unwrap_or_default();
        let template = format!("{{prefix:<{width}}} {{msg}} {{pos:>4}}/{{len}}");
        let style = ProgressStyle::with_template(&template)
            .expect("ICE: the overview template is malformed");

        let multi = MultiProgress::new();
        let mut bars = Vec::with_capacity(chains.len());
        let mut ranges = Vec::with_capacity(chains.len());

        for (index, chain) in chains.iter().enumerate() {
            let start = obligations
                .iter()
                .position(|obligation| obligation.chain == index)
                .unwrap_or(obligations.len());
            let count = obligations
                .iter()
                .filter(|obligation| obligation.chain == index)
                .count();
            ranges.push(start..start + count);

            let bar = multi.add(ProgressBar::new(count as u64));
            bar.set_style(style.clone());
            bar.set_prefix(chain.label());
            bars.push(bar);
        }

        Self {
            bars,
            ranges,
            _multi: multi,
        }
    }

    /// Redraws every line from `states`.
    ///
    /// Called by the provers as they pick work up and put it down, so the whole
    /// picture is rebuilt rather than patched — there is no ordering between
    /// threads to patch against.
    pub fn refresh(&self, states: &[AtomicU8]) {
        // The overview draws to stderr, so the colours are decided against it.
        // Styling is per symbol rather than through the template, since a line
        // carries one symbol per obligation and each has its own state.
        let base = Style::new().for_stderr();

        for (bar, range) in self.bars.iter().zip(self.ranges.iter()) {
            let mut symbols = String::new();
            let mut finished = 0;
            for index in range.clone() {
                let state = ProofState::from_u8(states[index].load(Ordering::Relaxed));
                symbols.push_str(&state.styled_symbol(base.clone()));
                if state.is_finished() {
                    finished += 1;
                }
            }
            bar.set_position(finished);
            bar.set_message(symbols);
        }
    }

    /// Leaves the finished lines on screen.
    pub fn finish(&self) {
        for bar in self.bars.iter() {
            bar.finish();
        }
    }
}

/// Prints the chain's stages, one line per pass.
pub fn print_chain(chain: &Chain) {
    let count = chain.stages.len();
    println!(
        "{} ({count} {}, heap size {})",
        chain.manifest.contract,
        if count == 1 { "stage" } else { "stages" },
        chain.manifest.heap_size,
    );

    // The codegen step is listed after the stages. It is not a manifest stage —
    // the manifest indexes only what the optimizer produced — but it is an
    // obligation, and these rows are what the overview's symbols line up
    // against, position for position.
    let has_codegen = !chain.stages.is_empty();

    let index_width = count.to_string().len();
    let from_width = column_width(
        chain
            .stages
            .iter()
            .map(|stage| stage.from.name())
            .chain(has_codegen.then_some(Format::NewYork.name())),
    );
    let to_width = column_width(
        chain
            .stages
            .iter()
            .map(|stage| stage.to.name())
            .chain(has_codegen.then_some(Format::Llvm.name())),
    );

    for stage in chain.stages.iter() {
        println!(
            "  {:>index_width$}  {:from_width$} -> {:to_width$}  {}",
            stage.index, stage.from, stage.to, stage.content.stage_id,
        );
    }

    if has_codegen {
        println!(
            "  {:>index_width$}  {:from_width$} -> {:to_width$}  {}",
            count,
            Format::NewYork,
            Format::Llvm,
            chain.llvm_ir_name.as_deref().unwrap_or(MISSING_STEP),
        );
    }
}

/// Prints what the overview's symbols mean, and the shape of the run.
pub fn print_legend(jobs: usize, obligations: usize, log_directory: Option<&Path>) {
    // Said before the run as well as after it, so that a long run does not
    // finish only to reveal that its evidence was never kept.
    if log_directory.is_none() {
        println!("{}", LOGS_DISCARDED_WARNING);
    }
    // The legend goes to stdout, which may be a different stream to the one the
    // overview draws on, so its colours are decided separately.
    let base = Style::new();
    println!(
        "{obligations} obligations, {jobs} at a time — \
         {} pending  {} running  {} proven correct  {} proven incorrect  {} unknown",
        ProofState::Pending.styled_symbol(base.clone()),
        ProofState::Running.styled_symbol(base.clone()),
        ProofState::ProvenCorrect.styled_symbol(base.clone()),
        ProofState::ProvenIncorrect.styled_symbol(base.clone()),
        ProofState::Unknown.styled_symbol(base),
    );
}

/// Prints the counterexamples and the overview of a run.
pub fn report(obligations: &[Obligation<'_>], run: &Run, log_directory: Option<&Path>) -> Totals {
    let mut totals = Totals::default();

    // The report goes to stdout, which may be a different stream to the one the
    // overview drew on, so its colours are decided separately.
    let base = Style::new();
    let failed = ProofState::ProvenIncorrect
        .style(base.clone())
        .bold()
        .apply_to("FAILED");

    for (index, verdict) in run.verdicts.iter() {
        totals.record(verdict);
        if let Verdict::NotEquivalent(counterexample) = verdict {
            let label = obligations
                .get(*index)
                .map(|obligation| obligation.label.as_str())
                .unwrap_or(CODEGEN_STEP);
            println!("  {failed}  {label}: {counterexample}");
        }
    }

    println!();
    println!(
        "{} obligations: {} proved, {} failed, {} unproven",
        totals.total(),
        styled_count(ProofState::ProvenCorrect, totals.proved, base.clone()),
        styled_count(ProofState::ProvenIncorrect, totals.failed, base.clone()),
        styled_count(ProofState::Unknown, totals.unproven, base.clone()),
    );
    match log_directory {
        Some(directory) => println!(
            "{} logs in {}",
            totals.total() - run.log_failures.len(),
            directory.display(),
        ),
        None => println!("{}", LOGS_DISCARDED_WARNING),
    }
    for failure in run.log_failures.iter() {
        println!("warning: {failure}");
    }

    if totals.failed == 0 && totals.unproven == 0 && totals.total() > 0 {
        let ok = ProofState::ProvenCorrect.style(base).bold();
        println!("{}", ok.apply_to("OK: every obligation holds"));
    }

    totals
}

/// A count, coloured for the state it counts, or dimmed when it is zero.
///
/// A red `0 failed` reads as a problem at a glance when it is the opposite of
/// one, so a zero carries no colour of its own.
fn styled_count(state: ProofState, count: usize, base: Style) -> String {
    let style = if count == 0 {
        base.dim()
    } else {
        state.style(base)
    };

    style.apply_to(count).to_string()
}

/// The width that lays `values` out in one column.
fn column_width<'a>(values: impl Iterator<Item = &'a str>) -> usize {
    values.map(str::len).max().unwrap_or_default()
}
