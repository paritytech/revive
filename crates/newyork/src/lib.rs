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

// Re-export main types
pub use from_yul::{TranslationError, YulTranslator};
pub use heap_opt::{
    AccessPattern, HeapAnalysis, HeapAnalysisStats, HeapOptResults, MemorySlot, OffsetInfo,
};
pub use inline::{
    analyze_call_graph, inline_functions, CallGraphAnalysis, InlineDecision, InlineResults,
};
pub use ir::{
    AddressSpace, BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOp, Value, ValueId,
};
pub use mem_opt::{MemOptResults, MemoryOptimizer};
pub use printer::{
    print_expr, print_function, print_object, print_statement, Printer, PrinterConfig,
};
pub use simplify::{
    deduplicate_functions, deduplicate_functions_fuzzy, fold_constant_keccak, Simplifier,
    SimplifyResults,
};
pub use ssa::SsaBuilder;
pub use to_llvm::{CodegenError, LlvmCodegen};
pub use type_inference::{TypeConstraint, TypeInference};
pub use validate::{validate_function, validate_object, ValidationError, ValidationResult};

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

    // Run optimization passes on the entire object tree (including subobjects).
    // Subobjects contain the deployed (runtime) contract code, which is where
    // most functions live and where optimizations have the biggest impact.
    let (inline_results, mem_opt_results) = optimize_object_tree(&mut ir_object);

    // Debug: print the IR before heap analysis
    if std::env::var("NEWYORK_DUMP_IR").is_ok() {
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

    // Run analysis passes on the full object tree
    let mut heap_analysis = HeapAnalysis::new();
    heap_analysis.analyze_object(&ir_object);
    let heap_opt = HeapOptResults::from_analysis(&heap_analysis);

    let mut type_info = TypeInference::new();
    type_info.infer_object_tree(&ir_object);

    // Iterative parameter and return type narrowing with full re-inference.
    // Each iteration: narrow params/returns → re-run full type inference with the new
    // types → refine call-site demands. The re-inference cascades forward
    // widths: narrowed params (I64) produce narrow add/and results. The demand
    // refinement ensures call-site arguments also get narrow demands.
    for _ in 0..4 {
        let mut changed = type_info.narrow_function_params(&mut ir_object);
        // Call-site forward narrowing: narrow params where ALL callers provide
        // narrow arguments (e.g., and(val, 2^160-1) for address validation).
        changed |= type_info.narrow_function_params_from_callers(&mut ir_object);
        // Forward-based return narrowing: narrow returns whose min_width < I256
        changed |= type_info.narrow_function_returns(&mut ir_object);
        // Backward demand-based return narrowing: narrow returns where ALL callers
        // only use narrow results (e.g., as memory offsets)
        changed |= type_info.narrow_function_returns_from_demand(&mut ir_object);
        if !changed {
            break;
        }
        // Re-run full type inference with narrowed param/return types.
        // The forward pass reads function.params (now narrowed), seeding
        // min_width at I64 instead of I256. This cascades through the
        // function body: add(I64, I8) → I65, and(I65, I256) → I65, etc.
        type_info = TypeInference::new();
        type_info.infer_object_tree(&ir_object);
        // Refine call-site demands with the narrowed param types.
        // This updates fn_arg_demand so argument values get narrow backward
        // demands, enabling further param narrowing in the next iteration.
        type_info.refine_demands_from_params(&ir_object);
    }

    // Validate IR correctness
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

/// Runs optimization passes on an object and all its subobjects recursively.
/// Returns the combined inline and memory optimization results from all objects.
fn optimize_object_tree(object: &mut ir::Object) -> (InlineResults, MemOptResults) {
    // Run inlining pass first - this exposes more optimization opportunities
    let mut inline_results = inline_functions(object);

    // Run simplification pass (constant folding, algebraic identities, copy propagation, DCE)
    // This cleans up the IR after inlining, eliminating redundant operations.
    let mut simplifier = Simplifier::new();
    let _simplify_stats = simplifier.simplify_object(object);

    // Run function deduplication after simplification (canonical forms are cleaner)
    let _dedup_count = deduplicate_functions(object);

    // Run fuzzy function deduplication: functions that differ only in literal constants
    // are merged by parameterizing the differing literals.
    let _fuzzy_dedup_count = simplify::deduplicate_functions_fuzzy(object);

    // Run memory optimization pass (load-after-store elimination)
    let mut mem_optimizer = MemoryOptimizer::new();
    let mut mem_opt_results = mem_optimizer.optimize_object(object);
    // Run FMP propagation pass (replace mload(0x40) with known constant)
    let mut fmp_prop = mem_opt::FmpPropagation::new(0);
    fmp_prop.propagate_object(object);

    // Fold constant keccak256 expressions created by the mem_opt pass.
    // The mem_opt pass creates Keccak256Single/Pair nodes from mstore+keccak256
    // patterns. When the argument is a constant, we can precompute the hash.
    simplify::fold_constant_keccak(object);

    // Second full simplify pass: mem_opt, FMP propagation, and keccak fold create new
    // constant expressions and dead code. A full simplify pass (constant folding, copy
    // propagation, algebraic simplification, DCE) propagates these opportunities through
    // downstream arithmetic, which DCE alone would miss.
    let mut simplifier2 = Simplifier::new();
    simplifier2.simplify_object(object);

    // Compound outlining: detect multi-statement patterns like
    // `let hash = keccak256_pair(key, slot); let val = sload(hash)` and replace
    // with compound IR nodes `mapping_sload(key, slot)` that get lowered to
    // keccak256_pair + sload/sstore calls, eliminating the intermediate hash value.
    compound_outlining::outline_compounds_in_object(object);

    // Guard narrowing: detect `if gt(val, MASK) { revert/panic }` patterns and
    // insert `val_narrow = and(val, MASK)` after the guard. This gives type
    // inference proof that the value fits in fewer bits, enabling downstream
    // narrowing of comparisons, arithmetic, and memory operations.
    guard_narrow::narrow_guards_in_object(object);

    // Third simplify pass: compound outlining and guard narrowing introduce new
    // constant expressions and dead code. A final simplify pass propagates these
    // opportunities and cleans up the IR before LLVM codegen.
    let mut simplifier3 = Simplifier::new();
    simplifier3.simplify_object(object);

    // Second dedup pass: guard narrowing and compound outlining canonicalize
    // code into forms that may expose new duplicate or near-duplicate functions.
    let _dedup_count2 = deduplicate_functions(object);
    let _fuzzy_dedup_count2 = simplify::deduplicate_functions_fuzzy(object);

    // Recursively optimize subobjects
    for subobject in &mut object.subobjects {
        let (sub_inline, sub_mem_opt) = optimize_object_tree(subobject);
        // Merge sub-results into main results
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
