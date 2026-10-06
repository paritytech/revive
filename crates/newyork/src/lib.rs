//! NEW Yul OptimizIR Kit (newyork)
//!
//! This crate provides a custom intermediate representation (IR) for the Revive
//! compiler, positioned between Yul and LLVM IR. It enables domain-specific
//! optimizations that LLVM cannot perform because it lacks semantic knowledge
//! of the PolkaVM target and EVM/Solidity source domains.
//!
//! # Architecture
//!
//! ```text
//! Yul AST ──► newyork IR ──► [Optimizations] ──► LLVM IR ──► RISC-V
//!          (from_yul)      (passes)           (to_llvm)
//! ```
//!
//! # Design Principles
//!
//! 1. **SSA with Structured Control Flow** - Preserves high-level structure from Yul
//! 2. **Explicit Types with Address Spaces** - Every value has a known bit-width
//! 3. **Pure Expressions vs Effectful Statements** - Enables easier reasoning
//! 4. **Semantic Annotations** - Storage/memory operations tagged with region info

pub mod export;
pub mod from_yul;
pub mod guard_narrow;
pub mod heap_opt;
pub mod inline;
pub mod ir;
pub mod mapping_access_outlining;
pub mod mem_opt;
pub mod printer;
pub mod serde_hex;
pub mod simplify;
pub mod ssa;
pub mod to_llvm;
pub mod type_inference;
pub mod validate;

pub use export::{
    CapturedStage, ManifestStage, SideTables, StageManifest, StageRecord, StageSink, StageWriter,
    PASS_FROM_YUL, STAGE_MANIFEST_SCHEMA,
};
pub use from_yul::{TranslationError, YulTranslator};
pub use heap_opt::{
    AccessPattern, HeapAnalysis, HeapAnalysisStats, HeapOptResults, MemorySlot, OffsetInfo,
};
pub use inline::{
    analyze_call_graph, inline_functions, CallGraphAnalysis, InlineDecision, InlineResults,
};
pub use ir::{
    AddressSpace, BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function,
    FunctionId, MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
    ValueId,
};
pub use mem_opt::{MemOptResults, MemoryOptimizer};
pub use printer::{
    print_expression, print_function, print_object, print_object_with_types, print_statement,
    Printer, PrinterConfig,
};
pub use simplify::{
    deduplicate_functions, deduplicate_functions_fuzzy, fold_constant_keccak, Simplifier,
    SimplifyResults,
};
pub use ssa::SsaBuilder;
pub use to_llvm::{CodegenError, LlvmCodegen};
pub use type_inference::{ResolvedType, ResolvedTypes, TypeConstraint, TypeInference};
pub use validate::{validate_object, ValidationError, ValidationResult};

/// Result of translating a Yul object to newyork IR.
pub struct TranslationResult {
    /// The translated IR object.
    pub object: Object,
    /// Heap optimization results (identifies where byte-swapping can be skipped).
    pub heap_opt: HeapOptResults,
    /// Type inference results (narrower types for values).
    pub type_info: TypeInference,
    /// Memory optimization results (load-after-store, dead store elimination).
    pub mem_opt: MemOptResults,
    /// Inlining results (which functions were inlined and removed).
    pub inline_results: InlineResults,
}

/// Translates a Yul object to newyork IR.
///
/// This is the main entry point for converting Yul AST to the new IR format.
/// Returns the IR object along with heap optimization analysis results.
///
/// # Example
///
/// ```ignore
/// use revive_newyork::translate_yul_object;
/// use revive_yul::parser::statement::object::Object;
///
/// let yul_object: Object = /* parse yul */;
/// let mut sink = StageSink::disabled();
/// let result = translate_yul_object(&yul_object, &mut sink, 131072)?;
/// let ir_object = result.object;
/// let heap_opt = result.heap_opt;
/// ```
///
/// `heap_size` is the configured EVM heap size in bytes. `sink` receives the IR
/// as it stands after every pass; pass [`StageSink::disabled`] when the caller
/// does not want stage artifacts.
pub fn translate_yul_object(
    yul_object: &revive_yul::parser::statement::object::Object,
    sink: &mut StageSink<'_>,
    heap_size: u64,
) -> Result<TranslationResult, TranslationError> {
    let mut translator = YulTranslator::new();
    let mut ir_object = translator.translate_object(yul_object)?;
    sink.capture(export::PASS_FROM_YUL, &ir_object, SideTables::none());

    let (mut inline_results, mem_opt_results) = optimize_object_tree(&mut ir_object, sink);

    let mut type_info = TypeInference::new();
    type_info.infer_object_tree(&ir_object);

    let type_info = type_inference::narrow_signatures_to_fixed_point(&mut ir_object, type_info);
    sink.capture(
        "narrow",
        &ir_object,
        SideTables {
            type_info: Some(&type_info),
            ..SideTables::none()
        },
    );

    let type_info = run_late_inline_loop(&mut ir_object, &mut inline_results, type_info, sink);

    let heap_opt = ir_object.analyze_heap(heap_size);
    sink.capture(
        "heap_opt",
        &ir_object,
        SideTables {
            type_info: Some(&type_info),
            heap_opt: Some(&heap_opt),
            ..SideTables::none()
        },
    );

    let (type_info, heap_opt) =
        reinfer_for_unbounded_fmp(&mut ir_object, type_info, heap_opt, heap_size);

    if let Err(errors) = validate::validate_object(&ir_object) {
        let details = errors
            .iter()
            .map(|error| format!("  - {error}"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "ICE: IR validation failed for object `{}` after optimization pipeline:\n{}",
            ir_object.name, details,
        );
    }

    // The IR as codegen will receive it. Byte-identical to `heap_opt` whenever the
    // free memory pointer is bounded, but the side tables differ: `heap_opt` runs
    // after the narrowing fixed point and can force a full re-inference.
    sink.capture(
        "final",
        &ir_object,
        SideTables {
            type_info: Some(&type_info),
            heap_opt: Some(&heap_opt),
            inline_results: Some(&inline_results),
            mem_opt: Some(&mem_opt_results),
        },
    );

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
        mem_opt: mem_opt_results,
        inline_results,
    })
}

/// Maximum number of late inline + simplify + narrow iterations.
///
/// One round captures every additional inline the cost model can make once the pipeline's other
/// passes have settled (measured on the OZ corpus — two or more rounds produce identical output).
/// The bound is kept as a constant so future cost-model work that depends on cascading shrinkage
/// across iterations doesn't have to re-introduce the scaffolding.
const LATE_INLINE_ITERATIONS: u32 = 1;

/// Runs a fixed-point loop of late inline + simplify + dedup + type narrow.
///
/// Now that parameter narrowing has propagated through the IR and simplification has folded any
/// newly exposed constants, some wrapper functions have shrunk below the inline thresholds. After
/// the early inline + heap + mem_opt + mapping_access_outlining + guard_narrow + first round of param
/// narrowing has completed, many wrapper helpers have collapsed to a handful of statements. The
/// early inliner couldn't act on them because they were still wrapped in pre-simplify noise;
/// running the inliner again at this point — with re-estimated function sizes and on top of the
/// narrowed signatures — picks up the residue.
///
/// This is intentionally separate from the early inliner: the early pass exposes intra-procedural
/// opportunities that drive the rest of the pipeline; the late pass collects the per-function
/// shrinkage produced by every subsequent optimization (mem_opt, mapping_access_outlining, guard_narrow,
/// full type narrowing).
///
/// Within the loop, compound outlining + guard narrowing are re-run after inlining because the
/// inlined bodies expose new keccak256_pair+sload pairs and overflow checks that the early pass
/// couldn't see across the call boundary. A final size refresh follows the loop so LLVM-level
/// inline hints (set during codegen) see the post-simplify, post-narrow shape rather than the
/// larger pre-cleanup estimates that `inline_functions` last set internally.
///
/// The function takes ownership of `type_info` so we can reseed it after the inlined IR is in
/// place; the returned `TypeInference` reflects the final state used for downstream codegen.
fn run_late_inline_loop(
    ir_object: &mut ir::Object,
    inline_results: &mut InlineResults,
    mut type_info: TypeInference,
    sink: &mut StageSink<'_>,
) -> TypeInference {
    for iteration in 0..LATE_INLINE_ITERATIONS {
        // Each iteration's stages are labelled `lateN.<pass>`, so raising
        // `LATE_INLINE_ITERATIONS` keeps them distinguishable on its own.
        sink.push_scope(format!("late{iteration}"));

        inline::estimate_function_sizes(ir_object);
        sink.capture("estimate_sizes", ir_object, SideTables::none());

        *inline_results += inline_functions(ir_object);
        sink.capture("inline", ir_object, SideTables::none());

        let mut simplifier = Simplifier::new();
        simplifier.simplify_object(ir_object);
        sink.capture("simplify", ir_object, SideTables::none());

        mapping_access_outlining::outline_mapping_accesses_in_object(ir_object);
        sink.capture("outline_mapping", ir_object, SideTables::none());

        guard_narrow::narrow_guards_in_object(ir_object);
        sink.capture("guard_narrow", ir_object, SideTables::none());

        let mut simplifier_post = Simplifier::new();
        simplifier_post.simplify_object(ir_object);
        sink.capture("simplify", ir_object, SideTables::none());

        let _ = deduplicate_functions(ir_object);
        sink.capture("dedup", ir_object, SideTables::none());

        let _ = simplify::deduplicate_functions_fuzzy(ir_object);
        sink.capture("dedup_fuzzy", ir_object, SideTables::none());

        type_info = TypeInference::new();
        type_info.infer_object_tree(ir_object);
        type_info = type_inference::narrow_signatures_to_fixed_point(ir_object, type_info);
        sink.capture(
            "narrow",
            ir_object,
            SideTables {
                type_info: Some(&type_info),
                ..SideTables::none()
            },
        );

        sink.pop_scope();
    }
    inline::estimate_function_sizes(ir_object);
    sink.capture("estimate_sizes", ir_object, SideTables::none());
    type_info
}

/// Re-infers types with `FreePointerSlot` `mload` widths forced to I256 when `heap_opt` reports the
/// FMP word may hold a non-bounded value.
///
/// `heap_opt` runs after the type-narrowing fixed point, so the first inference assumed the sound-
/// only-when-bounded I32 FMP width. When the FMP could be unbounded, codegen loads the full FMP word
/// (no `FMP < heap_size` range proof); the I32 forward width would then let a downstream comparison
/// or offset bare-truncate the live value. Re-inferring with the flag set keeps the load — and every
/// value derived from it — full width. The IR is already at its narrowing fixed point, so re-running
/// `narrow_signatures` only reproduces the same parameter types (it never un-narrows); `heap_opt` is
/// recomputed in case it shifted. Bounded FMPs (the Solidity allocator, all of OZ) skip this and keep
/// the I32 width.
fn reinfer_for_unbounded_fmp(
    ir_object: &mut ir::Object,
    type_info: TypeInference,
    heap_opt: HeapOptResults,
    heap_size: u64,
) -> (TypeInference, HeapOptResults) {
    if !heap_opt.fmp_could_be_unbounded() {
        return (type_info, heap_opt);
    }
    let mut reinferred = TypeInference::new();
    reinferred.set_fmp_could_be_unbounded(true);
    reinferred.infer_object_tree(ir_object);
    let reinferred = type_inference::narrow_signatures_to_fixed_point(ir_object, reinferred);
    let heap_opt = ir_object.analyze_heap(heap_size);
    (reinferred, heap_opt)
}

/// Runs the full newyork optimization pipeline on an object and its subobjects.
///
/// Pass order matters: inlining runs first to expose intra-procedural opportunities,
/// then simplify+dedup clean up the IR, then mem_opt + FMP propagation expose
/// constant keccak inputs, and finally compound outlining and guard narrowing
/// rewrite specialized patterns. Each of those rewriting passes can produce new
/// constants and dead code, so a simplify pass follows each cluster; a second dedup
/// catches near-duplicates that only emerge after canonicalization.
///
/// Subobjects are processed recursively and their inline/memory results are merged
/// into the returned aggregates.
fn optimize_object_tree(
    object: &mut ir::Object,
    sink: &mut StageSink<'_>,
) -> (InlineResults, MemOptResults) {
    let mut inline_results = inline_functions(object);
    sink.capture("inline", object, SideTables::none());

    let mut simplifier = Simplifier::new();
    let _simplify_stats = simplifier.simplify_object(object);
    sink.capture("simplify", object, SideTables::none());

    let _dedup_count = deduplicate_functions(object);
    sink.capture("dedup", object, SideTables::none());

    let _fuzzy_dedup_count = simplify::deduplicate_functions_fuzzy(object);
    sink.capture("dedup_fuzzy", object, SideTables::none());

    let mut mem_optimizer = MemoryOptimizer::new();
    let mut mem_opt_results = mem_optimizer.optimize_object(object);
    sink.capture("mem_opt", object, SideTables::none());

    let mut fmp_prop = mem_opt::FmpPropagation::new();
    fmp_prop.propagate_object(object);
    mem_opt_results.fmp_loads_eliminated += fmp_prop.loads_eliminated;
    sink.capture("fmp_prop", object, SideTables::none());

    simplify::fold_constant_keccak(object);
    sink.capture("fold_keccak", object, SideTables::none());

    let mut simplifier2 = Simplifier::new();
    simplifier2.simplify_object(object);
    sink.capture("simplify", object, SideTables::none());

    mapping_access_outlining::outline_mapping_accesses_in_object(object);
    sink.capture("outline_mapping", object, SideTables::none());

    guard_narrow::narrow_guards_in_object(object);
    sink.capture("guard_narrow", object, SideTables::none());

    let mut simplifier3 = Simplifier::new();
    simplifier3.simplify_object(object);
    sink.capture("simplify", object, SideTables::none());

    let _eliminated = inline::eliminate_constant_parameters(object);
    sink.capture("const_params", object, SideTables::none());
    if _eliminated > 0 {
        let mut simplifier_post_const = Simplifier::new();
        simplifier_post_const.simplify_object(object);
        sink.capture("simplify", object, SideTables::none());
    }

    let _predicted = inline::inline_by_shrink_prediction(object);
    sink.capture("shrink_predict", object, SideTables::none());
    if _predicted > 0 {
        let mut simplifier_post_predict = Simplifier::new();
        simplifier_post_predict.simplify_object(object);
        sink.capture("simplify", object, SideTables::none());
    }

    let _dedup_count2 = deduplicate_functions(object);
    sink.capture("dedup", object, SideTables::none());

    let _fuzzy_dedup_count2 = simplify::deduplicate_functions_fuzzy(object);
    sink.capture("dedup_fuzzy", object, SideTables::none());

    for subobject in &mut object.subobjects {
        let (subobject_inline_results, subobject_mem_opt_results) =
            optimize_object_tree(subobject, sink);
        inline_results += subobject_inline_results;
        mem_opt_results += subobject_mem_opt_results;
    }

    (inline_results, mem_opt_results)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::export::CapturedStage;
    use crate::export::SideTables;
    use crate::export::StageSink;
    use crate::ir::BitWidth;
    use crate::ir::Expression;
    use crate::ir::Statement;
    use crate::ir::Type;
    use crate::ir::ValueId;

    /// An object with a deploy and a runtime half, so the recursion into
    /// subobjects shows up in the stage sequence.
    fn object() -> ir::Object {
        let mut object = ir::Object::new("Test".to_owned());
        object.code.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: num::BigUint::from(42u32),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        object
            .subobjects
            .push(ir::Object::new("Test_deployed".to_owned()));
        object
    }

    /// Runs the pipeline over [`object`], returning the sink it filled.
    fn run_pipeline(sink: &mut StageSink<'_>) {
        let mut object = object();
        let (mut inline_results, _) = optimize_object_tree(&mut object, sink);
        let mut type_info = TypeInference::new();
        type_info.infer_object_tree(&object);
        sink.capture("narrow", &object, SideTables::none());
        run_late_inline_loop(&mut object, &mut inline_results, type_info, sink);
    }

    /// The pipeline's shape, asserted in full.
    ///
    /// A pass added without a matching `capture` changes this list, which is the
    /// point: the exported chain would otherwise gain a silent gap, and a
    /// consumer would be asked to attribute two passes' worth of change to one.
    /// When a pass is added, extend the expectation rather than relaxing it.
    ///
    /// Stages `000`-`014` are the deploy object and `015`-`029` the runtime one,
    /// which runs the same passes; only the stage record's `object` separates
    /// them. The two guarded simplifies after `const_params` and
    /// `shrink_predict` do not fire for this object and so emit no stage.
    #[test]
    fn the_stage_sequence_covers_every_pass() {
        let mut sink = StageSink::disabled();
        run_pipeline(&mut sink);

        let early = [
            "inline",
            "simplify",
            "dedup",
            "dedup_fuzzy",
            "mem_opt",
            "fmp_prop",
            "fold_keccak",
            "simplify",
            "outline_mapping",
            "guard_narrow",
            "simplify",
            "const_params",
            "shrink_predict",
            "dedup",
            "dedup_fuzzy",
        ];
        let late = [
            "late0.estimate_sizes",
            "late0.inline",
            "late0.simplify",
            "late0.outline_mapping",
            "late0.guard_narrow",
            "late0.simplify",
            "late0.dedup",
            "late0.dedup_fuzzy",
            "late0.narrow",
        ];
        let expected = early
            .iter()
            .chain(early.iter())
            .chain(["narrow"].iter())
            .chain(late.iter())
            .chain(["estimate_sizes"].iter())
            .enumerate()
            .map(|(ordinal, pass)| format!("{ordinal:03}.{pass}"))
            .collect::<Vec<_>>();

        assert_eq!(sink.stage_ids(), expected.as_slice());
    }

    /// Both halves of a contract run the same passes, so the object name is the
    /// only thing distinguishing their records.
    #[test]
    fn subobject_stages_name_their_own_object() {
        let objects = Rc::new(RefCell::new(Vec::new()));
        let mut sink = {
            let objects = Rc::clone(&objects);
            StageSink::new(Box::new(move |stage: CapturedStage<'_>| {
                objects
                    .borrow_mut()
                    .push((stage.stage_id.to_owned(), stage.object.to_owned()));
                Ok(())
            }))
        };
        run_pipeline(&mut sink);
        assert!(sink.take_error().is_none());

        let objects = objects.borrow();
        let named = |stage_id: &str| {
            objects
                .iter()
                .find(|(id, _)| id == stage_id)
                .map(|(_, object)| object.clone())
                .unwrap_or_else(|| panic!("no stage `{stage_id}`"))
        };
        assert_eq!(named("000.inline"), "Test");
        assert_eq!(named("015.inline"), "Test_deployed");
        // The late passes run on the root, which by then holds the optimized
        // subobject, so they are attributed to the root.
        assert_eq!(named("032.late0.inline"), "Test");
    }

    /// An unpaired `push_scope` would prefix every later stage.
    #[test]
    fn the_loop_scope_does_not_outlive_the_loop() {
        let mut sink = StageSink::disabled();
        run_pipeline(&mut sink);

        let last = sink.stage_ids().last().expect("stages were recorded");
        assert_eq!(last, "040.estimate_sizes");
    }
}
