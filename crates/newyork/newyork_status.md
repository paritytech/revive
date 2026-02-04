# NewYork IR Status

## Current State

The newyork IR pipeline is **functionally complete** with **heap optimization and bidirectional type inference integrated**.

### What Works
- Yul AST to newyork IR translation (`from_yul.rs`)
- newyork IR to LLVM IR codegen (`to_llvm.rs`)
- All 62 integration tests pass with `RESOLC_USE_NEWYORK=1`
- All resolc tests pass (both with and without newyork)
- Proper error handling for unsupported opcodes (CALLCODE, CODECOPY in runtime, EXTCODECOPY)
- Heap analysis runs on every compiled contract
- **Bidirectional type inference** (forward + backward constraint propagation)
- Heap optimization results used in codegen (native byte order for internal memory)
- Type inference infrastructure ready for function specialization

### Code Size Comparison

| Contract | YUL Pipeline | NewYork Pipeline | Diff |
|----------|--------------|------------------|------|
| Baseline | 864 bytes | 864 bytes | 0 |
| Flipper | 2154 bytes | 2154 bytes | 0 |
| ERC20 | 17813 bytes | 17813 bytes | 0 |
| FibonacciIterative | 1424 bytes | 1424 bytes | 0 |
| Computation | 2323 bytes | 2323 bytes | 0 |

**Summary**: Code sizes are identical between pipelines. The newyork IR pipeline produces equivalent output to the Yul pipeline. The optimization infrastructure is in place but cannot reduce code size without deeper changes (see "Why No Size Reduction" below).

## Pipeline Architecture

```
Yul AST ──► newyork IR ──► HeapAnalysis ──► TypeInference ──► LLVM IR Codegen
         (from_yul.rs)    (heap_opt.rs)   (type_inference.rs)  (to_llvm.rs)
                               │                  │                  │
                       HeapOptResults ────────────┼─────────────────►│
                                                  │                  │
                                    TypeInference ─────────────────►│
                                    (min_width + max_width)         │
                                    (use contexts)                  │
                                                                    ▼
                                              Values at 256-bit for runtime compat
                                              Narrow types possible for internal ops
```

## Type Inference

### Bidirectional Constraint Propagation

The type inference now performs both forward and backward analysis:

1. **Forward Pass** (min_width):
   - Propagates minimum required width from literals and operation results
   - Example: literal `42` needs only I8, literal `0x10000` needs I32
   - Addition may overflow by 1 bit, multiplication doubles width

2. **Backward Pass** (max_width):
   - Collects how each value is **used** via `UseContext` enum
   - Propagates constraints backward from use sites
   - Example: if value is only used as `mload(x)` offset, only 64 bits needed

### UseContext Enum

```rust
pub enum UseContext {
    MemoryOffset,    // mload/mstore offset → 64-bit sufficient
    MemoryValue,     // mstore value → 256-bit required (EVM compat)
    StorageAccess,   // sload/sstore → 256-bit required
    Comparison,      // lt/gt/eq → preserves narrow type
    Arithmetic,      // add/mul → may need full width
    FunctionArg,     // function call argument
    FunctionReturn,  // returned to caller → 256-bit (escapes)
    ExternalCall,    // external call args → 256-bit (EVM ABI)
    General,         // unknown use
}
```

### TypeConstraint Structure

```rust
pub struct TypeConstraint {
    pub min_width: BitWidth,  // Minimum from forward propagation
    pub max_width: BitWidth,  // Maximum from backward propagation
    pub is_signed: bool,      // Signed operation detected
}
```

### Why No Size Reduction Yet

The type inference cannot reduce code size because:

1. **Runtime function signatures**: Memory/storage/call functions expect 256-bit arguments
2. **Escaping values**: Any value that leaves the contract (return, call, log) needs 256-bit
3. **LLVM optimization**: LLVM already folds trivial zext/trunc of constants

To actually benefit from narrow types, we need:
- **Function specialization**: Generate variants of functions for narrow argument types
- **Runtime variants**: Provide `mstore32`, `mstore64` etc. for narrow offsets
- **ABI changes**: Use narrower types at internal function boundaries

## Heap Optimization

### How It Works

1. **Analysis Phase** (`heap_opt.rs`):
   - Tracks memory access patterns and offset information
   - Identifies "tainted" regions (unaligned writes, byte-level access)
   - Identifies "escaping" regions (return data, call data, log data)
   - Computes `native_safe_regions` and `native_safe_offsets`

2. **Codegen Phase** (`to_llvm.rs`):
   - Checks `HeapOptResults.all_native()` for whole-contract optimization
   - Uses native (non-byte-swapping) heap functions when all accesses are safe
   - Falls back to byte-swapping functions otherwise

### HeapOptResults

```rust
pub struct HeapOptResults {
    pub native_safe_regions: BTreeSet<u64>,
    pub native_safe_offsets: BTreeSet<u64>,
    pub total_accesses: usize,
    pub unknown_accesses: usize,
    pub tainted_count: usize,
    pub escaping_count: usize,
}

pub fn all_native(&self) -> bool {
    self.total_accesses > 0
        && self.unknown_accesses == 0
        && self.tainted_count == 0
        && self.escaping_count == 0
}
```

### Why Limited Heap Optimization Benefits

Most contracts don't benefit because:
- Memory regions that escape (return, call, log) must use big-endian
- The analysis is conservative - if unsure, it doesn't optimize
- Typical Flipper contract: `total=3, unknown=0, tainted=2, escaping=1`

## Files Modified

### Type Inference (Bidirectional)
1. **`crates/newyork/src/type_inference.rs`**:
   - Added `max_width` to `TypeConstraint`
   - Added `UseContext` enum for tracking value usage
   - Added `uses: BTreeMap<u32, BTreeSet<UseContext>>` to track all uses
   - Renamed `infer_*` to `infer_*_forward` for clarity
   - Added `collect_uses_*` methods for backward pass
   - Added `apply_backward_constraints()` to narrow max_width

### Heap Optimization
2. **`crates/newyork/src/heap_opt.rs`**: Added tracking fields, fixed `all_native()`
3. **`crates/llvm-context/src/polkavm/context/pointer/heap.rs`**: Native load/store
4. **`crates/llvm-context/src/polkavm/evm/memory.rs`**: `load_native()`, `store_native()`

### Codegen Integration
5. **`crates/newyork/src/to_llvm.rs`**:
   - Uses `type_info` from translation
   - Helper methods for type conversion ready but values kept at 256-bit
   - Conditional native heap function emission based on `all_native()`

### Resolc Integration
6. **`crates/resolc/src/project/contract/ir/newyork.rs`**: Pass analysis results to codegen

## Testing

Run all tests with newyork IR:
```bash
RESOLC_USE_NEWYORK=1 cargo test --package revive-integration
```

Run unit tests:
```bash
cargo test --package revive-newyork
```

Debug heap analysis (output written to `/tmp/resolc_heap_debug.log`):
```bash
rm -f /tmp/resolc_heap_debug.log
RESOLC_DEBUG_HEAP=1 RESOLC_USE_NEWYORK=1 resolc input.sol -o out --overwrite
cat /tmp/resolc_heap_debug.log
```

## Next Steps for Code Size Reduction

### Completed Analysis (2026-02)

Investigation revealed several findings:

1. **Dead function elimination**: Already works via LLVM optimization. Unused runtime functions are stripped.

2. **Comparison narrow-type optimization**: Code exists in `to_llvm.rs` (using `ensure_same_type` for comparisons) but has no effect because all values are stored at i256 (word type) for EVM compatibility.

3. **Per-offset heap optimization**: Not beneficial. Emitting both native and non-native heap functions would add ~200 bytes of code, while savings from skipping byte-swap are only ~40 bytes per occurrence.

4. **Heap all_native mode**: Working but rarely triggered. Most contracts have escaping memory regions (returns, calls, logs) that require big-endian byte order. Typical Flipper contract: `total=3, unknown=0, tainted=2, escaping=1`.

5. **Narrow literal generation**: Attempted but reverted. Runtime functions (`safe_truncate_int_to_xlen`) only accept XLEN (32-bit) or WORD (256-bit) types. Generating i8/i32/i64 literals causes assertion failures when passed to memory operations. The runtime API would need modification to support intermediate widths.

### Future Optimization Paths

To achieve code size reduction, deeper changes would be needed:

**Medium-term (Function Specialization)**:
- **Internal function narrowing**: Generate narrow-typed variants of internal functions
- **Call site specialization**: If caller always passes narrow values, use specialized callee
- **Store values at inferred type**: Change `Let` statement to store at narrow type instead of i256
- **Modify runtime API**: Add support for intermediate widths (i64) in `safe_truncate_int_to_xlen`

**Long-term (ABI Changes)**:
- **Narrow runtime functions**: `mstore32(offset32, value256)` variants
- **Internal ABI**: Use narrow types at internal function boundaries
- **Escape analysis**: Track which values truly need 256-bit

## Environment

Enable newyork pipeline: `export RESOLC_USE_NEWYORK=1`

Build: `make install-bin` (requires `LLVM_SYS_181_PREFIX` set to LLVM 18.1.x)

Test: `RESOLC_USE_NEWYORK=1 cargo test --package revive-integration`

Debug: `RESOLC_DEBUG_HEAP=1` to see heap analysis statistics
