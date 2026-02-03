# NewYork IR Status

## Current State

The newyork IR pipeline is **functionally complete** but **not yet optimizing**.

### What Works
- Yul AST → newyork IR translation (`from_yul.rs`)
- newyork IR → LLVM IR codegen (`to_llvm.rs`)
- All integration tests pass with `RESOLC_USE_NEWYORK=1`
- All resolc tests pass
- Proper error handling for unsupported opcodes (CALLCODE, CODECOPY in runtime, EXTCODECOPY)

### Code Size Comparison (vs main branch)

| Contract | NewYork | Main | Diff |
|----------|---------|------|------|
| Baseline | 864 | 864 | 0 |
| Computation | 2323 | 2323 | 0 |
| DivisionArithmetics | 14475 | 14475 | 0 |
| **ERC20** | **17813** | **17229** | **+584 (+3.4%)** |
| Events | 1663 | 1663 | 0 |
| FibonacciIterative | 1424 | 1421 | +3 |
| Flipper | 2154 | 2154 | 0 |
| SHA1 | 7675 | 7751 | -76 (-1%) |

**Summary**: ERC20 is 3.4% larger - this is a regression, not an improvement.

## Why No Improvement?

The heap optimization pass exists (`heap_opt.rs`) but is **never called** in the pipeline:

```
from_yul.rs  →  IR  →  to_llvm.rs
                ↑
         (no optimization passes run here!)
```

The current pipeline is a 1:1 translation with no optimizations applied. The regression comes from IR translation overhead without optimization benefit.

## What Needs To Be Done

### 1. Integrate Heap Optimization Pass
The `HeapAnalysis` struct in `heap_opt.rs` analyzes memory access patterns to:
- Identify aligned vs unaligned accesses
- Track which memory regions escape to external calls
- Determine where byte-swapping can be eliminated

This analysis is implemented but never used. Need to:
1. Run `HeapAnalysis` on IR after `from_yul.rs` translation
2. Store analysis results in the IR (e.g., annotate `MemoryRegion` on MLoad/MStore)
3. Use results in `to_llvm.rs` to skip byte-swapping for aligned non-escaping accesses

### 2. Additional Optimization Passes (Future)
- Dead code elimination
- Constant propagation/folding
- Common subexpression elimination
- Function inlining for small functions

## Files

- `src/lib.rs` - Module exports
- `src/ir.rs` - IR data structures (Object, Function, Block, Statement, Expr, etc.)
- `src/from_yul.rs` - Yul AST → newyork IR translation
- `src/to_llvm.rs` - newyork IR → LLVM IR codegen
- `src/ssa.rs` - SSA value tracking during translation
- `src/heap_opt.rs` - Heap optimization analysis (NOT YET INTEGRATED)
- `src/type_inference.rs` - Type width inference for narrowing

## Recent Fixes (this session)

1. **Error message format** - Changed `#[error("Unsupported operation: {0}")]` to `#[error("{0}")]` to match expected test output

2. **Unsupported opcode checks** - Added proper checks for CALLCODE, CODECOPY (runtime), EXTCODECOPY with correct error messages

3. **Undefined ValueId bug** - When If/Switch have ALL branches terminate early (via Leave/Break/Return/Revert), outputs were never set. Fixed by setting outputs to `undef` values.

4. **LLVM "terminator in middle of block"** - After adding unreachable terminators, need to create a dead block for subsequent code.

## Environment

Enable newyork pipeline: `export RESOLC_USE_NEWYORK=1`

Requires solc 0.8.33+ (older versions produce different Yul that may cause issues).
