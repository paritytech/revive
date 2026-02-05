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
//!
//! For now, allow missing docs while the crate is in development.
#![allow(missing_docs)]
#![deny(clippy::all)]

pub mod from_yul;
pub mod heap_opt;
pub mod ir;
pub mod printer;
pub mod ssa;
pub mod to_llvm;
pub mod type_inference;
pub mod validate;

// Re-export main types
pub use from_yul::{TranslationError, YulTranslator};
pub use heap_opt::{
    AccessPattern, HeapAnalysis, HeapAnalysisStats, HeapOptResults, MemorySlot, OffsetInfo,
};
pub use ir::{
    AddressSpace, BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOp, Value, ValueId,
};
pub use printer::{
    print_expr, print_function, print_object, print_statement, Printer, PrinterConfig,
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
    let ir_object = translator.translate_object(yul_object)?;

    // Run heap analysis to identify optimization opportunities
    let mut heap_analysis = HeapAnalysis::new();
    heap_analysis.analyze_object(&ir_object);

    // Build optimization results
    let heap_opt = HeapOptResults::from_analysis(&heap_analysis);

    // Run type inference to determine minimum bit-widths
    let mut type_info = TypeInference::new();
    type_info.infer_object(&ir_object);

    // Validate IR correctness in debug builds
    #[cfg(debug_assertions)]
    {
        if let Err(errors) = validate::validate_object(&ir_object) {
            for error in &errors {
                log::warn!("IR validation error in {}: {}", ir_object.name, error);
            }
        }
    }

    // Log analysis statistics in debug builds
    #[cfg(debug_assertions)]
    {
        let stats = heap_analysis.statistics();
        if stats.total_accesses > 0 {
            log::debug!(
                "Heap analysis for {}: {} accesses ({} aligned, {} static), {} tainted regions, {} escaping regions, {} native-safe regions",
                ir_object.name,
                stats.total_accesses,
                stats.aligned_accesses,
                stats.static_accesses,
                stats.tainted_regions,
                stats.escaping_regions,
                heap_opt.native_safe_regions.len()
            );
        }
        let type_constraints = type_info.constraints().len();
        if type_constraints > 0 {
            log::debug!(
                "Type inference for {}: {} values with constraints",
                ir_object.name,
                type_constraints
            );
        }
    }

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
    })
}
