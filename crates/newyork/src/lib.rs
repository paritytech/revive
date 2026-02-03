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
pub mod ssa;
pub mod to_llvm;
pub mod type_inference;

// Re-export main types
pub use from_yul::{TranslationError, YulTranslator};
pub use heap_opt::{AccessPattern, HeapAnalysis, HeapAnalysisStats, MemorySlot, OffsetInfo};
pub use ir::{
    AddressSpace, BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOp, Value, ValueId,
};
pub use ssa::SsaBuilder;
pub use to_llvm::{CodegenError, LlvmCodegen};
pub use type_inference::{TypeConstraint, TypeInference};

/// Translates a Yul object to newyork IR.
///
/// This is the main entry point for converting Yul AST to the new IR format.
///
/// # Example
///
/// ```ignore
/// use revive_newyork::translate_yul_object;
/// use revive_yul::parser::statement::object::Object;
///
/// let yul_object: Object = /* parse yul */;
/// let ir_object = translate_yul_object(&yul_object)?;
/// ```
pub fn translate_yul_object(
    yul_object: &revive_yul::parser::statement::object::Object,
) -> Result<Object, TranslationError> {
    let mut translator = YulTranslator::new();
    translator.translate_object(yul_object)
}
