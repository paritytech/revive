# NewYork IR Status

## Current State

The newyork IR pipeline is **functionally complete** with **heap optimization and type inference fully integrated**.

### What Works
- Yul AST to newyork IR translation (`from_yul.rs`)
- newyork IR to LLVM IR codegen (`to_llvm.rs`)
- All 62 integration tests pass with `RESOLC_USE_NEWYORK=1`
- All resolc tests pass
- Proper error handling for unsupported opcodes (CALLCODE, CODECOPY in runtime, EXTCODECOPY)
- Heap analysis runs on every compiled contract
- Type inference runs on every compiled contract
- Analysis results are used in codegen

### Code Size Comparison (vs main branch)

| Contract | NewYork | Main | Diff |
|----------|---------|------|------|
| Baseline | 606 | 606 | 0 |
| Computation | 1941 | 1941 | 0 |
| DivisionArithmetics | 14454 | 14454 | 0 |
| ERC20 | 19222 | 19222 | 0 |
| Events | 1309 | 1309 | 0 |
| FibonacciIterative | 1192 | 1192 | 0 |
| SHA1 | 14448 | 14448 | 0 |

**Summary**: Code sizes are identical. Both optimization passes are integrated but LLVM's existing optimizations eliminate any redundant operations.

## Pipeline Architecture

```
Yul AST ──► newyork IR ──► HeapAnalysis ──► TypeInference ──► LLVM IR Codegen
         (from_yul.rs)    (heap_opt.rs)   (type_inference.rs)  (to_llvm.rs)
                               │                  │                  │
                       HeapOptResults ────────────┼─────────────────►│
                                                  │                  │
                                         TypeInference ─────────────►│
                                         (narrower types)            │
                                                                     ▼
                                                        Values stored at inferred width
                                                        Extended to 256-bit when needed
```

## Heap Optimization

### How It Works

1. **Analysis Phase** (`heap_opt.rs`):
   - Tracks memory access patterns and offset information
   - Identifies "tainted" regions (unaligned writes, byte-level access)
   - Identifies "escaping" regions (return data, call data, log data, etc.)
   - Computes `native_safe_regions` and `native_safe_offsets`

2. **Codegen Phase** (`to_llvm.rs`):
   - For MLoad/MStore operations, checks if offset is a compile-time constant
   - Queries `HeapOptResults.can_use_native(offset)`
   - Uses native (non-byte-swapping) functions when safe

### Why No Heap Optimization Benefits Yet

Most Solidity contracts don't benefit because:
- Memory regions that escape to external code (return, call, log) must use big-endian
- The analysis is conservative - if unsure, it doesn't optimize

## Type Inference

### How It Works

1. **Analysis Phase** (`type_inference.rs`):
   - Dataflow-based analysis determining minimum bit-width for each SSA value
   - Tracks constraints from literals, arithmetic, comparisons, EVM builtins
   - Runs to fixed point to find narrowest safe type per value
   - Tracks signedness for signed operations

2. **Codegen Phase** (`to_llvm.rs`):
   - Values stored at inferred width (may be i1, i8, i32, i64, i160, or i256)
   - Automatically zero-extended to 256-bit when needed for:
     - Memory operations (MStore, MStore8)
     - Storage operations (SStore, TStore, SLoad, TLoad)
     - Other EVM operations expecting 256-bit values

### Why No Type Inference Benefits Yet

Code sizes remain identical because:
- Values are extended back to 256-bit for most operations
- LLVM's optimization passes eliminate the redundant extend/truncate pairs
- The infrastructure is in place for future optimizations that keep values narrow throughout

### Potential Future Improvements

- Use narrower types in arithmetic operations (not just storage)
- Avoid extending for operations that don't need 256-bit (e.g., address comparisons)
- Generate narrower PHI nodes and function parameters

## New Runtime Functions

Added to `revive-llvm-context`:

- `__revive_load_heap_word_native` - Load without byte-swapping
- `__revive_store_heap_word_native` - Store without byte-swapping

## Files Modified

### Heap Optimization
1. **`crates/llvm-context/src/polkavm/context/pointer/heap.rs`**: Native load/store functions
2. **`crates/llvm-context/src/polkavm/evm/memory.rs`**: `load_native()`, `store_native()` wrappers
3. **`crates/llvm-context/src/polkavm/context/mod.rs`**: `build_load_native()`, `build_store_native()` methods
4. **`crates/llvm-context/src/lib.rs`**: Exported native function types

### Type Inference Integration
5. **`crates/newyork/src/lib.rs`**: Added `type_info` to `TranslationResult`, run type inference
6. **`crates/newyork/src/type_inference.rs`**: Added `Clone` derive
7. **`crates/newyork/src/to_llvm.rs`**:
   - Added `type_info` field to `LlvmCodegen`
   - Added `inferred_width()`, `int_type_for_width()`, `ensure_word_type()`, `convert_to_inferred_type()` helpers
   - Let bindings store values at inferred width
   - MStore/MStore8 ensure 256-bit values
   - `value_to_argument()` ensures 256-bit for storage ops

### Resolc Integration
8. **`crates/resolc/src/project/contract/ir/newyork.rs`**: Pass `type_info` to codegen

## Testing

Run all tests with newyork IR:
```bash
RESOLC_USE_NEWYORK=1 cargo test --package revive-integration
```

Run unit tests:
```bash
cargo test --package revive-newyork
```

## Next Steps (Optional Enhancements)

1. **More aggressive type inference usage**: Keep values narrow through arithmetic operations
2. **More aggressive heap analysis**: Track data flow to identify more internal-only regions
3. **Additional optimization passes**: DCE, constant propagation, CSE
4. **Function specialization**: Generate narrow-typed variants of functions

## Environment

Enable newyork pipeline: `export RESOLC_USE_NEWYORK=1`

Build: `make install-bin` (requires `LLVM_SYS_181_PREFIX` set to LLVM 18.1.x installation)

Test: `RESOLC_USE_NEWYORK=1 cargo test --package revive-integration`
