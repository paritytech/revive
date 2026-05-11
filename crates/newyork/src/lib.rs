//! NEW Yul OptimziR Kit (newyork)
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
//!
//! For now, allow missing docs while the crate is in development.
#![allow(missing_docs)]
#![deny(clippy::all)]

/// Environment variable: when set, dumps the newyork IR for every translated object to
/// `/tmp/newyork_ir_<object>.txt` after optimization passes have run.
pub const NEWYORK_DUMP_IR_ENV: &str = "NEWYORK_DUMP_IR";

pub mod compound_outlining;
pub mod from_yul;
pub mod guard_narrow;
pub mod heap_opt;
pub mod inline;
pub mod ir;
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
    print_expression, print_function, print_object, print_statement, Printer, PrinterConfig,
};
pub use simplify::{
    deduplicate_functions, deduplicate_functions_fuzzy, fold_constant_keccak, Simplifier,
    SimplifyResults,
};
pub use ssa::SsaBuilder;
pub use to_llvm::{CodegenError, LlvmCodegen};
pub use type_inference::{TypeConstraint, TypeInference};
pub use validate::{validate_function, validate_object, ValidationError, ValidationResult};
/// Maximum number of param/return narrowing iterations.
///
/// Each iteration narrows function signatures, re-runs full type inference with the
/// new widths so narrowed parameters cascade through `add`/`and`/etc. forward, and
/// then refines call-site demands. Four iterations is enough to reach a fixed point
/// on the OZ corpus; any further work is bounded by an explicit `changed` check.
const PARAM_NARROW_ITERATIONS: u32 = 4;

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
/// let result = translate_yul_object(&yul_object)?;
/// let ir_object = result.object;
/// let heap_opt = result.heap_opt;
/// ```
pub fn translate_yul_object(
    yul_object: &revive_yul::parser::statement::object::Object,
) -> Result<TranslationResult, TranslationError> {
    let mut translator = YulTranslator::new();
    let mut ir_object = translator.translate_object(yul_object)?;

    let (inline_results, mem_opt_results) = optimize_object_tree(&mut ir_object);

    if std::env::var(NEWYORK_DUMP_IR_ENV).is_ok() {
        use std::io::Write;
        let dump_path = format!("/tmp/newyork_ir_{}.txt", ir_object.name.replace('/', "_"));
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&dump_path)
        {
            let _ = write!(f, "{}", print_object(&ir_object));
        }
    }

    let mut heap_analysis = HeapAnalysis::new();
    heap_analysis.analyze_object(&ir_object);
    let heap_opt = HeapOptResults::from_analysis(&heap_analysis);

    let mut type_info = TypeInference::new();
    type_info.infer_object_tree(&ir_object);

    let type_info = narrow_signatures_to_fixed_point(&mut ir_object, type_info);

    if let Err(errors) = validate::validate_object(&ir_object) {
        for error in &errors {
            log::warn!("IR validation error in {}: {}", ir_object.name, error);
        }
    }

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
        mem_opt: mem_opt_results,
        inline_results,
    })
}

/// Iteratively narrows function parameter and return types until no change.
///
/// Each iteration applies four narrowing strategies — forward param, caller-driven
/// param, forward return, demand-driven return — and re-runs full type inference
/// so the new signature widths cascade through every function body before the next
/// iteration. Bounded by [`PARAM_NARROW_ITERATIONS`] but exits early on a fixed point.
fn narrow_signatures_to_fixed_point(
    ir_object: &mut ir::Object,
    mut type_info: TypeInference,
) -> TypeInference {
    for _ in 0..PARAM_NARROW_ITERATIONS {
        let mut changed = type_info.narrow_function_params(ir_object);
        changed |= type_info.narrow_function_params_from_callers(ir_object);
        changed |= type_info.narrow_function_returns(ir_object);
        changed |= type_info.narrow_function_returns_from_demand(ir_object);
        if !changed {
            break;
        }
        type_info = TypeInference::new();
        type_info.infer_object_tree(ir_object);
        type_info.refine_demands_from_params(ir_object);
    }
    type_info
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
    let mut fmp_prop = mem_opt::FmpPropagation::new(0);
    fmp_prop.propagate_object(object);

    simplify::fold_constant_keccak(object);

    let mut simplifier2 = Simplifier::new();
    simplifier2.simplify_object(object);

    compound_outlining::outline_compounds_in_object(object);
    guard_narrow::narrow_guards_in_object(object);

    let mut simplifier3 = Simplifier::new();
    simplifier3.simplify_object(object);

    let _dedup_count2 = deduplicate_functions(object);
    let _fuzzy_dedup_count2 = simplify::deduplicate_functions_fuzzy(object);

    for subobject in &mut object.subobjects {
        let (sub_inline, sub_mem_opt) = optimize_object_tree(subobject);
        inline_results.inlined_call_sites += sub_inline.inlined_call_sites;
        inline_results
            .removed_functions
            .extend(sub_inline.removed_functions);
        inline_results.decisions.extend(sub_inline.decisions);
        mem_opt_results.loads_eliminated += sub_mem_opt.loads_eliminated;
        mem_opt_results.stores_eliminated += sub_mem_opt.stores_eliminated;
        mem_opt_results.values_tracked += sub_mem_opt.values_tracked;
        mem_opt_results.keccak_pairs_fused += sub_mem_opt.keccak_pairs_fused;
        mem_opt_results.keccak_singles_fused += sub_mem_opt.keccak_singles_fused;
    }

    (inline_results, mem_opt_results)
}
