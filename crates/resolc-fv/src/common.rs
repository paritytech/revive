//! The vocabulary the rest of the crate is written in.
//!
//! One chain of compiler artifacts, the steps it is made of, and what a prover
//! concluded about each step.

use std::collections::BTreeMap;
use std::path::PathBuf;

use console::Style;
use revive_common::EXTENSION_NEWYORK_MANIFEST;
use revive_newyork::ir;
use revive_newyork::StageManifest;
use revive_newyork::StageRecord;
use revive_yul::parser::statement::object;

/// The label the codegen obligation is reported under, since it is not a stage
/// and so has no stage identifier of its own.
pub const CODEGEN_STEP: &str = "codegen";

/// What a log calls the side of a step that is not there.
pub const MISSING_STEP: &str = "(nothing)";

/// The codegen dump the chain's last obligation is stated against.
///
/// The unoptimized dump is codegen's direct output. The optimized one has
/// already been through LLVM's own passes, which is a separate obligation this
/// tool does not state.
pub const LLVM_DUMP_SUFFIX: &str = "unoptimized";

/// The IR format a pass reads or writes.
///
/// Every obligation is stated between two formats, and the two are the same for
/// all but the first and last steps of a chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// The Yul AST, which is what the chain starts from.
    Yul,
    /// The newyork IR.
    NewYork,
    /// The LLVM IR codegen lowers the newyork IR to, which is where the chain ends.
    Llvm,
}

impl Format {
    /// The name the format prints as.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Yul => "yul",
            Self::NewYork => "newyork",
            Self::Llvm => "llvm",
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `pad` rather than `write_str`, so that a width in the format string
        // lays the formats out in a column instead of being ignored.
        formatter.pad(self.name())
    }
}

/// One pass of the chain, with the IR it produced.
///
/// A stage is read as a transformation: it consumed [`Self::from`] and produced
/// [`Self::to`], and [`Self::content`] is what came out. The obligation for the
/// pass is stated between this stage's content and the previous stage's.
#[derive(Clone, Debug)]
pub struct Stage {
    /// Position in the chain, which is the order the passes ran in.
    pub index: usize,
    /// The format the pass read.
    pub from: Format,
    /// The format the pass wrote.
    pub to: Format,
    /// The IR after the pass, with the side tables codegen reads.
    pub content: StageRecord<'static>,
}

/// The IR on one side of an obligation.
#[derive(Clone, Copy, Debug)]
pub enum Content<'a> {
    /// The Yul AST the chain starts from.
    Yul(&'a object::Object),
    /// The newyork IR a pass produced.
    NewYork(&'a ir::Object),
    /// The LLVM IR codegen emitted, as its textual form.
    Llvm(&'a str),
}

/// Everything parsed out of one contract's artifacts.
pub struct Chain {
    /// The manifest's path, which also locates the codegen dumps beside it.
    pub manifest_path: PathBuf,
    /// The manifest itself.
    pub manifest: StageManifest<'static>,
    /// The stages it indexes, in pipeline order.
    pub stages: Vec<Stage>,
    /// The Yul AST the chain starts from, when it was dumped.
    pub yul_ast: Option<object::Object>,
    /// The LLVM IR codegen emitted, when it was dumped.
    pub llvm_ir: Option<String>,
    /// The file name that dump was read from, for a log to point at.
    pub llvm_ir_name: Option<String>,
}

impl Chain {
    /// The file-name stem every artifact of this contract shares.
    ///
    /// The proof logs are named from it, so a log sits next to the artifacts it
    /// is about under the same name.
    pub fn stem(&self) -> &str {
        self.manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(&format!(".{EXTENSION_NEWYORK_MANIFEST}")))
            .unwrap_or("chain")
    }

    /// What the overview calls this chain.
    ///
    /// The manifest's file name is the compiler's escaped form of the source
    /// path — `%5F` for an underscore, `_` for a separator — which is built to
    /// survive a file system rather than to be read. The manifest records the
    /// contract identifier as it was written, so that is what gets shown, cut
    /// down to the source file and the contract within it to keep the line
    /// narrow.
    pub fn label(&self) -> String {
        let contract = self.manifest.contract.as_ref();
        let (source_path, name) = match contract.rsplit_once(':') {
            Some((source_path, name)) => (source_path, Some(name)),
            None => (contract, None),
        };

        let file = source_path
            .rsplit_once('/')
            .map(|(_, file)| file)
            .unwrap_or(source_path);

        match name {
            Some(name) => format!("{file}:{name}"),
            None => file.to_owned(),
        }
    }
}

/// One proof invocation.
pub struct Obligation<'a> {
    /// The chain this belongs to, as an index into the parsed chains.
    pub chain: usize,
    /// What the overview calls it: a stage identifier, or the codegen step.
    pub label: String,
    /// What the step read, named so a reader can go and open it: the preceding
    /// stage, the Yul AST the chain starts from, or nothing.
    pub source: String,
    /// What the step wrote: the stage it produced, or the codegen dump.
    pub target: String,
    /// The format the step read.
    pub from: Format,
    /// The format the step wrote.
    pub to: Format,
    /// The two sides, or the reason the obligation cannot be stated at all.
    ///
    /// An obligation with no stateable sides still appears in the overview. A
    /// step that was silently dropped would leave the run looking more complete
    /// than it is.
    pub sides: Result<(Content<'a>, Content<'a>), String>,
}

/// Where one proof invocation has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofState {
    /// Planned, not yet started.
    Pending,
    /// A prover is working on it.
    Running,
    /// The prover showed the step preserves meaning.
    ProvenCorrect,
    /// The prover found a counterexample.
    ProvenIncorrect,
    /// The prover returned nothing either way.
    Unknown,
}

impl ProofState {
    /// The symbol the overview shows for this state.
    pub fn symbol(self) -> char {
        match self {
            Self::Pending => '\u{00b7}',
            Self::Running => '\u{25b8}',
            Self::ProvenCorrect => '\u{2713}',
            Self::ProvenIncorrect => '\u{2717}',
            Self::Unknown => '?',
        }
    }

    /// The colour this state is shown in, on the stream `base` points at.
    ///
    /// Shared with the summary, so that a count and the symbols it counts are
    /// always the same colour.
    ///
    /// `console` decides whether to emit the escapes at all, from whether that
    /// stream is a terminal that takes colour and from `NO_COLOR` and friends.
    /// That is the same check `indicatif` makes for its own drawing, so the two
    /// cannot disagree.
    pub fn style(self, base: Style) -> Style {
        match self {
            Self::Pending => base.dim(),
            Self::Running => base.cyan(),
            Self::ProvenCorrect => base.green(),
            Self::ProvenIncorrect => base.red(),
            Self::Unknown => base.yellow(),
        }
    }

    /// The symbol, coloured for the stream `base` points at.
    pub fn styled_symbol(self, base: Style) -> String {
        self.style(base).apply_to(self.symbol()).to_string()
    }

    /// Whether the invocation has finished, whatever it concluded.
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Self::ProvenCorrect | Self::ProvenIncorrect | Self::Unknown
        )
    }

    /// The encoding the overview's shared state uses.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Running => 1,
            Self::ProvenCorrect => 2,
            Self::ProvenIncorrect => 3,
            Self::Unknown => 4,
        }
    }

    /// Restores a state from [`Self::as_u8`].
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Pending,
            1 => Self::Running,
            2 => Self::ProvenCorrect,
            3 => Self::ProvenIncorrect,
            _ => Self::Unknown,
        }
    }
}

/// A witness that a pass did not preserve the program's meaning.
#[derive(Clone, Debug)]
pub struct CounterExample {
    /// What the two sides disagree on.
    pub description: String,
}

impl std::fmt::Display for CounterExample {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.description)
    }
}

/// The outcome of one equivalence obligation.
#[derive(Clone, Debug)]
pub enum Verdict {
    /// The step was shown to preserve the program's meaning.
    Equivalent,
    /// The step changed the program's meaning, with a witness.
    NotEquivalent(CounterExample),
    /// Nothing was shown either way, with the reason.
    ///
    /// Deliberately distinct from [`Self::Equivalent`]. A verifier that reported
    /// an obligation it never examined as holding would be worse than one that
    /// reported nothing, because the output would read as a proof.
    Unproven(String),
}

impl Verdict {
    /// The word a log header uses for this verdict.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Equivalent => "proved",
            Self::NotEquivalent(_) => "refuted",
            Self::Unproven(_) => "unknown",
        }
    }

    /// The state the overview shows for this verdict.
    pub fn state(&self) -> ProofState {
        match self {
            Self::Equivalent => ProofState::ProvenCorrect,
            Self::NotEquivalent(_) => ProofState::ProvenIncorrect,
            Self::Unproven(_) => ProofState::Unknown,
        }
    }
}

/// What a checker produced: what it concluded, and what it recorded getting there.
///
/// The transcript is kept even when the obligation was never attempted, so that
/// every obligation has a log saying what became of it.
pub struct Proof {
    /// What the checker concluded.
    pub verdict: Verdict,
    /// The checker's transcript.
    pub log: String,
}

impl Proof {
    /// Puts `note` at the head of the transcript.
    ///
    /// Used for what the prover did around the check itself, which belongs in
    /// the log but is not the checker's own account of its reasoning.
    pub fn with_note(mut self, note: String) -> Self {
        self.log.insert_str(0, &note);
        self
    }

    /// A proof that was never attempted, recording why.
    pub fn unproven(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            log: format!("result: unknown\nreason: {reason}\n"),
            verdict: Verdict::Unproven(reason),
        }
    }
}

/// What one invocation produced.
pub struct Run {
    /// Verdicts by obligation index, which is pipeline order.
    pub verdicts: BTreeMap<usize, Verdict>,
    /// Logs that could not be written, as messages.
    ///
    /// Collected rather than printed where they happen: a prover writing to the
    /// terminal would tear the live overview.
    pub log_failures: Vec<String>,
}

/// Counts of the obligations in a run.
#[derive(Debug, Default)]
pub struct Totals {
    /// Obligations discharged.
    pub proved: usize,
    /// Obligations that failed, each with a counterexample.
    pub failed: usize,
    /// Obligations nothing was shown about.
    pub unproven: usize,
}

impl Totals {
    /// Counts `verdict`.
    pub fn record(&mut self, verdict: &Verdict) {
        match verdict {
            Verdict::Equivalent => self.proved += 1,
            Verdict::NotEquivalent(_) => self.failed += 1,
            Verdict::Unproven(_) => self.unproven += 1,
        }
    }

    /// The number of obligations counted.
    pub fn total(&self) -> usize {
        self.proved + self.failed + self.unproven
    }
}
