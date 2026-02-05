# newyork Crate Status Summary

**Date:** 2026-02-05
**Crate Name:** revive-newyork (NEW Yul OptimziR Kit)

## Overview

The newyork crate implements a custom intermediate representation (IR) between Yul and LLVM IR, enabling domain-specific optimizations that LLVM cannot perform due to lack of semantic knowledge about PolkaVM and EVM/Solidity domains.

## Implementation Progress vs IR_PLAN.md

### Phase 0: Infrastructure Setup - COMPLETE

| Item | Status | Notes |
|------|--------|-------|
| Create crate structure | Done | `crates/newyork/` with all core modules |
| IR data structures | Done | `ir.rs` - full type system, SSA values, statements, expressions |
| Visitor traits | Partial | Not a formal visitor, but analysis traversal in heap_opt.rs and type_inference.rs |
| Pretty printer | Not started | No IR printer for debugging yet |
| IR validation | Not started | No SSA well-formedness or type checking passes |

### Phase 0.5: IR Round-Trip Validation - NOT STARTED

| Item | Status | Notes |
|------|--------|-------|
| Yul -> IR translation | Done | `from_yul.rs` - complete translation of all Yul constructs |
| IR -> Yul printer | Not started | No round-trip testing capability |
| Round-trip test suite | Not started | Cannot verify IR correctness via round-trip |

### Phase 1: Pass-Through to LLVM - IN PROGRESS

| Item | Status | Notes |
|------|--------|-------|
| IR -> LLVM lowering | Mostly done | `to_llvm.rs` - ~2700 lines, comprehensive implementation |
| Integration with pipeline | Unknown | Need to verify integration with resolc driver |
| All tests passing | Unknown | Need validation run |

### Phase 2: Type Inference - PARTIAL

| Item | Status | Notes |
|------|--------|-------|
| Type inference algorithm | Done | `type_inference.rs` - forward + backward dataflow |
| Insert type conversions | Not yet | Narrowed types computed but not used in codegen |
| Narrower LLVM types | Partial | Type info available but unclear if used |

### Phase 3: Memory Optimization - PARTIAL

| Item | Status | Notes |
|------|--------|-------|
| Memory analysis pass | Done | `heap_opt.rs` - alignment and escape analysis |
| Load-after-store elimination | Not started | Analysis only, no transformations |
| Dead store elimination | Not started | |
| Scratch-to-stack promotion | Not started | |

### Phase 4: Custom Inliner - NOT STARTED

- Call graph construction: Not started
- Cost-benefit analysis: Not started (but `size_estimate` field exists in Function)
- Inline transformation: Not started

### Phase 5: Pattern Rewrites - NOT STARTED

- No pattern matcher framework
- No rewrite rules implemented

## Module Status Details

### `ir.rs` - Core IR Data Structures
**Status: Complete**

Implements:
- `BitWidth` enum: I1, I8, I32, I64, I160, I256
- `AddressSpace` enum: Heap, Stack, Storage, Code
- `Type` enum: Int, Ptr, Void
- `MemoryRegion` enum: Scratch, FreePointerSlot, Dynamic, Unknown
- `ValueId`, `Value` - SSA value representation
- `BinOp`, `UnaryOp` - all EVM operations
- `Expr` enum - all pure expressions (literals, binary/ternary/unary ops, EVM builtins, loads)
- `Statement` enum - all effectful operations (stores, control flow, calls, etc.)
- `Region`, `Block` - structured control flow containers
- `Function`, `Object` - top-level constructs

Key design choices:
- SSA with structured control flow (MLIR SCF style)
- Explicit region inputs/outputs for If/Switch/For
- Memory region annotations for optimization
- Static slot tracking for storage operations

### `ssa.rs` - SSA Builder
**Status: Complete**

Provides:
- Fresh value ID allocation
- Scope management (enter/exit)
- Variable definition and lookup
- Modified variable tracking for control flow
- Scope merging for phi nodes

### `from_yul.rs` - Yul to IR Translation
**Status: Complete**

Translates all Yul constructs:
- Variable declarations and assignments
- All EVM builtins (arithmetic, bitwise, memory, storage, calls, etc.)
- Control flow: if, switch, for loops
- Function definitions with return value tracking
- Nested blocks and scopes
- Factory dependencies and immutables
- Proper SSA form with explicit yields for control flow joins

Notable implementation details:
- Two-pass translation: collect functions first, then translate bodies
- Return variable tracking for `leave` statements
- Combined scope for for-loop body and post regions
- Size estimation for inlining decisions

### `to_llvm.rs` - LLVM Code Generation
**Status: Mostly Complete (~2700 lines)**

Implements:
- Full IR to LLVM translation
- Integration with `PolkaVMContext` from revive-llvm-context
- All statement types
- All expression types
- Control flow (if, switch, for loops)
- Function declarations and calls
- External calls and contract creation
- Memory and storage operations
- Data operations (codecopy, calldatacopy, etc.)
- Immutable variables
- Heap optimization integration (uses `HeapOptResults`)
- Type inference integration (uses `TypeInference`)

### `type_inference.rs` - Type Inference Pass
**Status: Complete but Not Fully Utilized**

Implements:
- `TypeConstraint` with min_width/max_width/is_signed
- Forward dataflow for minimum width
- Backward dataflow for maximum width from use sites
- Use context classification (MemoryOffset, StorageAccess, Comparison, etc.)
- Width inference for all expressions and statements

Inference rules implemented:
- Literals: minimum width that fits
- Comparisons: I1 result
- Address builtins: I160
- Memory sizes: I64
- Arithmetic: width propagation with widening

### `heap_opt.rs` - Heap Optimization Analysis
**Status: Complete Analysis, No Transformations**

Implements:
- Memory access pattern classification (aligned vs unaligned, static vs dynamic)
- Offset tracking with alignment information
- Tainted region detection (unaligned writes, external data)
- Escape analysis (external calls, returns, logs)
- `HeapOptResults` for codegen integration

Used for:
- Determining where byte-swapping can be skipped
- Native byte order optimization opportunities

## Dependencies

```toml
[dependencies]
anyhow, log, num, thiserror, inkwell
revive-yul, revive-llvm-context, revive-common
```

## Key Gaps for Production Readiness

1. **No IR Validation** - Cannot verify SSA well-formedness or type consistency
2. **No Pretty Printer** - Difficult to debug IR transformations
3. **No Round-Trip Testing** - Cannot verify translation correctness
4. **Type Inference Not Applied** - Narrowed types computed but not used in LLVM codegen
5. **No Optimization Passes** - Analysis exists but no transformations
6. **Integration Unknown** - Need to verify integration with resolc driver

## Recommended Next Steps

1. **Validation & Testing**
   - Add IR pretty printer for debugging
   - Add IR validation pass (SSA dominance, type checking)
   - Add round-trip test: Yul -> IR -> (simple Yul-like output)
   - Run retester to verify correctness

2. **Type Inference Integration**
   - Use inferred types in LLVM codegen
   - Insert explicit truncate/extend operations
   - Verify no semantic changes via differential testing

3. **Memory Optimizations**
   - Implement load-after-store elimination using heap analysis
   - Implement dead store elimination

4. **Inlining**
   - Build call graph
   - Implement single-call function inlining
   - Use existing size_estimate for cost model

## Code Quality

- Uses `BTreeMap` throughout (deterministic iteration)
- Comprehensive doc comments on public items
- No magic numbers - uses module constants
- Meaningful, non-abbreviated identifiers
- Clean separation of concerns between modules
