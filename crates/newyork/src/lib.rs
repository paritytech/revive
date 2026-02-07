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

pub mod from_yul;
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
pub use simplify::{deduplicate_functions, Simplifier, SimplifyResults};
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
    let inline_results = optimize_object_tree(&mut ir_object);

    // Run analysis passes on the full object tree
    let mut heap_analysis = HeapAnalysis::new();
    heap_analysis.analyze_object(&ir_object);
    let heap_opt = HeapOptResults::from_analysis(&heap_analysis);

    let mut type_info = TypeInference::new();
    type_info.infer_object(&ir_object);

    // Also analyze subobjects
    for subobject in &ir_object.subobjects {
        heap_analysis.analyze_object(subobject);
        type_info.infer_object(subobject);
    }

    // Narrow function parameter types based on usage within function bodies.
    // This enables LLVM function signatures with smaller types, eliminating
    // overflow checks when parameters are used only as memory offsets.
    type_info.narrow_function_params(&mut ir_object);

    // Validate IR correctness in debug builds
    #[cfg(debug_assertions)]
    {
        if let Err(errors) = validate::validate_object(&ir_object) {
            for error in &errors {
                log::warn!("IR validation error in {}: {}", ir_object.name, error);
            }
        }
    }

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
        mem_opt: MemOptResults::default(),
        inline_results,
    })
}

/// Runs optimization passes on an object and all its subobjects recursively.
/// Returns the combined inline results from all objects.
fn optimize_object_tree(object: &mut ir::Object) -> InlineResults {
    // Run inlining pass first - this exposes more optimization opportunities
    let mut inline_results = inline_functions(object);

    // Run simplification pass (constant folding, algebraic identities, copy propagation, DCE)
    // This cleans up the IR after inlining, eliminating redundant operations.
    let mut simplifier = Simplifier::new();
    let _simplify_stats = simplifier.simplify_object(object);

    // Run function deduplication after simplification (canonical forms are cleaner)
    let _dedup_count = deduplicate_functions(object);

    // Run memory optimization pass (load-after-store elimination)
    let mut mem_optimizer = MemoryOptimizer::new();
    mem_optimizer.optimize_object(object);

    // Run FMP propagation pass (replace mload(0x40) with known constant)
    let mut fmp_prop = mem_opt::FmpPropagation::new(0);
    fmp_prop.propagate_object(object);

    // Recursively optimize subobjects
    for subobject in &mut object.subobjects {
        let sub_results = optimize_object_tree(subobject);
        // Merge sub-results into main results
        inline_results.inlined_call_sites += sub_results.inlined_call_sites;
        inline_results
            .removed_functions
            .extend(sub_results.removed_functions);
        inline_results.decisions.extend(sub_results.decisions);
    }

    inline_results
}
