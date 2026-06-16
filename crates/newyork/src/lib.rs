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
//!
//! # Modules
//!
//! - [`ir`] - Core IR data structures (types, values, statements, expressions)
//! - [`ssa`] - SSA builder for variable tracking and phi-node insertion
//! - [`from_yul`] - Translation from Yul AST to newyork IR
//! - [`to_llvm`] - LLVM code generation from newyork IR
//! - [`type_inference`] - Type inference pass for narrowing integer widths
//! - [`heap_opt`] - Heap optimization for partial big-endian emulation
//! - [`mem_opt`] - Memory optimization (load-after-store, dead store elimination)
//! - [`inline`] - Function inlining with custom heuristics for PolkaVM
#![deny(clippy::all)]

pub mod from_yul;
pub mod guard_narrow;
pub mod heap_opt;
pub mod inline;
pub mod ir;
pub mod mapping_access_outlining;
pub mod mem_opt;
pub mod printer;
pub mod simplify;
pub mod ssa;
pub mod to_llvm;
pub mod type_inference;
pub mod validate;

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
pub use type_inference::{TypeConstraint, TypeInference};
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
    /// The IR printed after the intra-object optimization passes, before the late passes.
    /// `Some` only when `capture_ir_snapshot` was requested.
    pub ir_snapshot: Option<String>,
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
/// let result = translate_yul_object(&yul_object, false)?;
/// let ir_object = result.object;
/// let heap_opt = result.heap_opt;
/// ```
pub fn translate_yul_object(
    yul_object: &revive_yul::parser::statement::object::Object,
    capture_ir_snapshot: bool,
) -> Result<TranslationResult, TranslationError> {
    let mut translator = YulTranslator::new();
    let mut ir_object = translator.translate_object(yul_object)?;

    let (mut inline_results, mem_opt_results) = optimize_object_tree(&mut ir_object);

    let ir_snapshot = capture_ir_snapshot.then(|| print_object(&ir_object));

    let mut type_info = TypeInference::new();
    type_info.infer_object_tree(&ir_object);

    let type_info = type_inference::narrow_signatures_to_fixed_point(&mut ir_object, type_info);

    let type_info = run_late_inline_loop(&mut ir_object, &mut inline_results, type_info);

    let heap_opt = ir_object.analyze_heap();
    let (type_info, heap_opt) = reinfer_for_unbounded_fmp(&mut ir_object, type_info, heap_opt);

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

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
        mem_opt: mem_opt_results,
        inline_results,
        ir_snapshot,
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
) -> TypeInference {
    for _ in 0..LATE_INLINE_ITERATIONS {
        inline::estimate_function_sizes(ir_object);
        *inline_results += inline_functions(ir_object);

        let mut simplifier = Simplifier::new();
        simplifier.simplify_object(ir_object);

        mapping_access_outlining::outline_mapping_accesses_in_object(ir_object);
        guard_narrow::narrow_guards_in_object(ir_object);
        let mut simplifier_post = Simplifier::new();
        simplifier_post.simplify_object(ir_object);

        let _ = deduplicate_functions(ir_object);
        let _ = simplify::deduplicate_functions_fuzzy(ir_object);

        type_info = TypeInference::new();
        type_info.infer_object_tree(ir_object);
        type_info = type_inference::narrow_signatures_to_fixed_point(ir_object, type_info);
    }
    inline::estimate_function_sizes(ir_object);
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
) -> (TypeInference, HeapOptResults) {
    if !heap_opt.fmp_could_be_unbounded() {
        return (type_info, heap_opt);
    }
    let mut reinferred = TypeInference::new();
    reinferred.set_fmp_could_be_unbounded(true);
    reinferred.infer_object_tree(ir_object);
    let reinferred = type_inference::narrow_signatures_to_fixed_point(ir_object, reinferred);
    let heap_opt = ir_object.analyze_heap();
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
fn optimize_object_tree(object: &mut ir::Object) -> (InlineResults, MemOptResults) {
    let mut inline_results = inline_functions(object);

    let mut simplifier = Simplifier::new();
    let _simplify_stats = simplifier.simplify_object(object);

    let _dedup_count = deduplicate_functions(object);
    let _fuzzy_dedup_count = simplify::deduplicate_functions_fuzzy(object);

    let mut mem_optimizer = MemoryOptimizer::new();
    let mut mem_opt_results = mem_optimizer.optimize_object(object);
    let mut fmp_prop = mem_opt::FmpPropagation::new();
    fmp_prop.propagate_object(object);
    mem_opt_results.fmp_loads_eliminated += fmp_prop.loads_eliminated;

    simplify::fold_constant_keccak(object);

    let mut simplifier2 = Simplifier::new();
    simplifier2.simplify_object(object);

    mapping_access_outlining::outline_mapping_accesses_in_object(object);
    guard_narrow::narrow_guards_in_object(object);

    let mut simplifier3 = Simplifier::new();
    simplifier3.simplify_object(object);

    let _eliminated = inline::eliminate_constant_parameters(object);
    if _eliminated > 0 {
        let mut simplifier_post_const = Simplifier::new();
        simplifier_post_const.simplify_object(object);
    }

    let _predicted = inline::inline_by_shrink_prediction(object);
    if _predicted > 0 {
        let mut simplifier_post_predict = Simplifier::new();
        simplifier_post_predict.simplify_object(object);
    }

    let _dedup_count2 = deduplicate_functions(object);
    let _fuzzy_dedup_count2 = simplify::deduplicate_functions_fuzzy(object);

    for subobject in &mut object.subobjects {
        let (subobject_inline_results, subobject_mem_opt_results) = optimize_object_tree(subobject);
        inline_results += subobject_inline_results;
        mem_opt_results += subobject_mem_opt_results;
    }

    (inline_results, mem_opt_results)
}
