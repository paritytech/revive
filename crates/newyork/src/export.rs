//! Export of the IR as it stands between optimization passes.
//!
//! The pipeline calls [`StageSink::capture`] after every pass. Each call produces
//! one self-contained record: the IR at that point plus the side tables codegen
//! will read, which the IR alone does not determine. An external consumer — the
//! formal-verification tooling this exists for — reconstructs the pass chain from
//! the records and discharges one equivalence obligation per adjacent pair.
//!
//! This module performs no I/O. The crate sits below `revive-llvm-context` in the
//! dependency order, so it hands serialized records to a writer supplied by the
//! caller, which owns the output directory and the file naming.

use revive_common::Keccak256;
use serde::Serialize;

use crate::heap_opt::HeapOptResults;
use crate::inline::InlineResults;
use crate::ir::Object;
use crate::mem_opt::MemOptResults;
use crate::type_inference::ResolvedTypes;
use crate::type_inference::TypeInference;

/// The number of digits in a stage ordinal.
///
/// Wide enough that the emitted file names sort lexically in pipeline order for
/// every object tree the compiler produces; a contract's deploy and runtime
/// objects together currently reach the high tens.
const ORDINAL_DIGITS: usize = 3;

/// The separator between a stage identifier's ordinal, scope and pass name.
const STAGE_ID_SEPARATOR: char = '.';

/// The side tables a stage carries alongside the IR.
///
/// Codegen reads the heap and type analyses in addition to the IR itself, so two
/// stages whose IR is byte-identical can still lower differently. A record that
/// omitted them would describe a transformation the consumer cannot reproduce.
/// Each is absent at the stages that run before the analysis producing it.
#[derive(Clone, Copy, Default)]
pub struct SideTables<'a> {
    /// Inferred value widths, from the narrowing fixed point.
    pub type_info: Option<&'a TypeInference>,
    /// Heap access analysis, which decides where byte-swapping is skipped.
    pub heap_opt: Option<&'a HeapOptResults>,
    /// Inlining decisions, which codegen turns into LLVM inline hints.
    pub inline_results: Option<&'a InlineResults>,
    /// Memory optimization counters.
    pub mem_opt: Option<&'a MemOptResults>,
}

impl<'a> SideTables<'a> {
    /// No side tables, for the stages that run before any analysis.
    pub fn none() -> Self {
        Self::default()
    }
}

/// One captured stage, as handed to a [`StageWriter`].
///
/// The JSON is the artifact's contents; the remaining fields are the metadata a
/// caller needs to name the file and index it without parsing the JSON back.
pub struct CapturedStage<'a> {
    /// Ordinal, optional scope and pass name, e.g. `007.fold_keccak` or
    /// `019.late0.simplify`. Unique within one translation.
    pub stage_id: &'a str,
    /// The pass that produced this IR. Stable across pipeline edits that shift
    /// ordinals, and the identity a consumer should key on.
    pub pass: &'a str,
    /// The Yul object this stage belongs to. A contract's deploy and runtime
    /// objects run the same pass sequence, and this is what separates them.
    pub object: &'a str,
    /// Hash of the serialized IR alone, excluding the side tables and metadata.
    /// Equal hashes across adjacent stages of one object mean the pass left the
    /// IR untouched — though it may still have moved a side table.
    pub ir_hash: &'a str,
    /// The serialized [`StageRecord`].
    pub json: &'a str,
}

/// Receives each captured stage. Supplied by the caller, which owns the output.
pub type StageWriter<'a> = Box<dyn FnMut(CapturedStage<'_>) -> anyhow::Result<()> + 'a>;

/// The contents of one stage artifact.
#[derive(Serialize)]
struct StageRecord<'a> {
    /// See [`CapturedStage::stage_id`].
    stage_id: &'a str,
    /// See [`CapturedStage::pass`].
    pass: &'a str,
    /// See [`CapturedStage::object`].
    object: &'a str,
    /// The IR as it stands after `pass`.
    ir: &'a Object,
    /// See [`SideTables::type_info`]. Resolved to the view codegen reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    type_info: Option<ResolvedTypes>,
    /// See [`SideTables::heap_opt`].
    #[serde(skip_serializing_if = "Option::is_none")]
    heap_opt: Option<&'a HeapOptResults>,
    /// See [`SideTables::inline_results`].
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_results: Option<&'a InlineResults>,
    /// See [`SideTables::mem_opt`].
    #[serde(skip_serializing_if = "Option::is_none")]
    mem_opt: Option<&'a MemOptResults>,
}

/// Collects the IR between optimization passes.
///
/// One sink spans a whole translation, including the recursion into subobjects,
/// so ordinals are unique across the object tree and no two stages can be written
/// to the same file.
pub struct StageSink<'a> {
    /// Where captured stages go. Absent when export is off, in which case the
    /// sink still records stage identifiers but serializes nothing.
    writer: Option<StageWriter<'a>>,
    /// The ordinal the next capture will take.
    ordinal: u32,
    /// The label inserted between ordinal and pass name, set while inside a loop.
    scope: Option<String>,
    /// Every stage identifier produced so far, in order.
    stage_ids: Vec<String>,
    /// The first failure, if any. Reported once at the end rather than aborting
    /// the compilation partway through a debug-only export.
    error: Option<anyhow::Error>,
}

impl<'a> StageSink<'a> {
    /// Creates a sink that writes every captured stage to `writer`.
    pub fn new(writer: StageWriter<'a>) -> Self {
        Self {
            writer: Some(writer),
            ordinal: 0,
            scope: None,
            stage_ids: Vec::new(),
            error: None,
        }
    }

    /// Creates a sink that serializes nothing.
    ///
    /// Stage identifiers are still recorded, which costs one short string per
    /// pass and lets a test assert the pipeline's shape without any output
    /// directory. This is what the compiler uses when export is not requested.
    pub fn disabled() -> Self {
        Self {
            writer: None,
            ordinal: 0,
            scope: None,
            stage_ids: Vec::new(),
            error: None,
        }
    }

    /// Inserts `label` into the stage identifiers until [`Self::pop_scope`].
    ///
    /// Used around a loop body so that each iteration's stages are distinguishable
    /// — `019.late0.simplify` against `027.late1.simplify`. Every push must be
    /// paired with a pop; an unpaired one shows up immediately in the stage list.
    pub fn push_scope(&mut self, label: impl Into<String>) {
        self.scope = Some(label.into());
    }

    /// Ends the scope opened by [`Self::push_scope`].
    pub fn pop_scope(&mut self) {
        self.scope = None;
    }

    /// Records the IR as it stands after `pass`.
    pub fn capture(&mut self, pass: &str, object: &Object, side_tables: SideTables<'_>) {
        let stage_id = self.next_stage_id(pass);
        self.stage_ids.push(stage_id.clone());

        if self.writer.is_none() || self.error.is_some() {
            return;
        }

        // The IR is serialized a second time here so that the hash covers it
        // alone, leaving the side tables and the metadata out. The duplication is
        // confined to the export path, which only runs when stages are requested.
        let ir_hash = match serde_json::to_string(object) {
            Ok(ir_json) => Keccak256::from_slice(ir_json.as_bytes()).to_string(),
            Err(error) => return self.fail(error.into()),
        };

        let record = StageRecord {
            stage_id: &stage_id,
            pass,
            object: &object.name,
            ir: object,
            type_info: side_tables.type_info.map(TypeInference::resolved_types),
            heap_opt: side_tables.heap_opt,
            inline_results: side_tables.inline_results,
            mem_opt: side_tables.mem_opt,
        };
        let json = match serde_json::to_string(&record) {
            Ok(json) => json,
            Err(error) => return self.fail(error.into()),
        };

        let captured = CapturedStage {
            stage_id: &stage_id,
            pass,
            object: &object.name,
            ir_hash: &ir_hash,
            json: &json,
        };
        let writer = self
            .writer
            .as_mut()
            .expect("the absent case returned above");
        if let Err(error) = writer(captured) {
            self.error = Some(error);
        }
    }

    /// The stage identifiers produced so far, in pipeline order.
    pub fn stage_ids(&self) -> &[String] {
        &self.stage_ids
    }

    /// Takes the first failure, leaving the sink without one.
    pub fn take_error(&mut self) -> Option<anyhow::Error> {
        self.error.take()
    }

    /// Allocates the next stage identifier and advances the ordinal.
    fn next_stage_id(&mut self, pass: &str) -> String {
        let ordinal = self.ordinal;
        self.ordinal += 1;
        match self.scope.as_deref() {
            Some(scope) => format!(
                "{ordinal:0width$}{STAGE_ID_SEPARATOR}{scope}{STAGE_ID_SEPARATOR}{pass}",
                width = ORDINAL_DIGITS,
            ),
            None => format!(
                "{ordinal:0width$}{STAGE_ID_SEPARATOR}{pass}",
                width = ORDINAL_DIGITS,
            ),
        }
    }

    /// Records the first failure and suppresses the rest.
    fn fail(&mut self, error: anyhow::Error) {
        self.error = Some(error);
    }
}

impl Default for StageSink<'_> {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::ir::BitWidth;
    use crate::ir::Expression;
    use crate::ir::Statement;
    use crate::ir::Type;
    use crate::ir::ValueId;

    /// One stage as seen by the test writer.
    #[derive(Clone)]
    struct Written {
        stage_id: String,
        pass: String,
        object: String,
        ir_hash: String,
        json: String,
    }

    /// A sink writing into a shared buffer, plus the buffer.
    fn recording_sink() -> (StageSink<'static>, Rc<RefCell<Vec<Written>>>) {
        let written = Rc::new(RefCell::new(Vec::new()));
        let sink = {
            let written = Rc::clone(&written);
            StageSink::new(Box::new(move |stage: CapturedStage<'_>| {
                written.borrow_mut().push(Written {
                    stage_id: stage.stage_id.to_owned(),
                    pass: stage.pass.to_owned(),
                    object: stage.object.to_owned(),
                    ir_hash: stage.ir_hash.to_owned(),
                    json: stage.json.to_owned(),
                });
                Ok(())
            }))
        };
        (sink, written)
    }

    fn object(name: &str) -> Object {
        let mut object = Object::new(name.to_owned());
        object.code.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: num::BigUint::from(42u32),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        object
    }

    #[test]
    fn stage_ids_carry_the_ordinal_and_the_scope() {
        let mut sink = StageSink::disabled();
        let object = object("Test");

        sink.capture("from_yul", &object, SideTables::none());
        sink.capture("inline", &object, SideTables::none());
        sink.push_scope("late0");
        sink.capture("inline", &object, SideTables::none());
        sink.capture("simplify", &object, SideTables::none());
        sink.pop_scope();
        sink.capture("final", &object, SideTables::none());

        assert_eq!(
            sink.stage_ids(),
            [
                "000.from_yul",
                "001.inline",
                "002.late0.inline",
                "003.late0.simplify",
                "004.final",
            ],
        );
    }

    #[test]
    fn a_disabled_sink_records_identifiers_but_writes_nothing() {
        let mut sink = StageSink::disabled();
        sink.capture("inline", &object("Test"), SideTables::none());

        assert_eq!(sink.stage_ids().len(), 1);
        assert!(sink.take_error().is_none());
    }

    #[test]
    fn a_record_carries_the_ir_and_the_side_tables() {
        let (mut sink, written) = recording_sink();
        let object = object("Test");
        let heap_opt = HeapOptResults::default();

        sink.capture(
            "heap_opt",
            &object,
            SideTables {
                heap_opt: Some(&heap_opt),
                ..SideTables::none()
            },
        );

        let written = written.borrow();
        let [stage] = written.as_slice() else {
            panic!("expected exactly one stage, got {}", written.len());
        };
        assert_eq!(stage.stage_id, "000.heap_opt");
        assert_eq!(stage.pass, "heap_opt");
        assert_eq!(stage.object, "Test");
        assert!(stage.ir_hash.starts_with("0x"), "{}", stage.ir_hash);

        let parsed: serde_json::Value =
            serde_json::from_str(&stage.json).expect("the record should parse");
        assert_eq!(parsed["stage_id"], "000.heap_opt");
        assert_eq!(parsed["ir"]["name"], "Test");
        assert!(parsed["heap_opt"].is_object(), "{}", stage.json);
        // Absent side tables are omitted rather than written as nulls.
        assert!(parsed.get("type_info").is_none(), "{}", stage.json);
        assert!(parsed.get("mem_opt").is_none(), "{}", stage.json);
    }

    /// The hash must answer "did this pass change the IR", so a side table
    /// appearing between two stages of identical IR must not move it.
    #[test]
    fn the_hash_covers_the_ir_alone() {
        let (mut sink, written) = recording_sink();
        let object = object("Test");
        let heap_opt = HeapOptResults::default();

        sink.capture("before", &object, SideTables::none());
        sink.capture(
            "after",
            &object,
            SideTables {
                heap_opt: Some(&heap_opt),
                ..SideTables::none()
            },
        );

        let written = written.borrow();
        assert_eq!(written[0].ir_hash, written[1].ir_hash, "the hash moved");
        assert_ne!(
            written[0].json, written[1].json,
            "the records are identical"
        );
    }

    #[test]
    fn a_changed_ir_moves_the_hash() {
        let (mut sink, written) = recording_sink();
        let mut object = object("Test");

        sink.capture("before", &object, SideTables::none());
        object.code.statements.clear();
        sink.capture("after", &object, SideTables::none());

        let written = written.borrow();
        assert_ne!(written[0].ir_hash, written[1].ir_hash);
    }

    #[test]
    fn the_first_write_failure_is_reported_once() {
        let attempts = Rc::new(RefCell::new(0usize));
        let mut sink = {
            let attempts = Rc::clone(&attempts);
            StageSink::new(Box::new(move |_| {
                *attempts.borrow_mut() += 1;
                Err(anyhow::anyhow!("the disk is full"))
            }))
        };
        let object = object("Test");

        sink.capture("inline", &object, SideTables::none());
        sink.capture("simplify", &object, SideTables::none());

        assert_eq!(*attempts.borrow(), 1, "the writer was called again");
        assert_eq!(
            sink.stage_ids().len(),
            2,
            "identifiers stopped being recorded"
        );
        let error = sink.take_error().expect("the failure should be reported");
        assert_eq!(error.to_string(), "the disk is full");
        assert!(
            sink.take_error().is_none(),
            "the failure was reported twice"
        );
    }
}
