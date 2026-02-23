# Optimization Findings - Agent Two

## Summary
The newyork optimizer provides significant codesize reduction for OpenZeppelin contracts (28-38%). However, there are specific optimization opportunities that could yield additional gains.

## Bytecode Size Comparison (NewYork vs Baseline)

| Contract | Baseline | NewYork | Reduction |
|----------|----------|---------|-----------|
| oz_gov.sol | 147712 | 105448 | 28.6% |
| oz_rwa.sol | 79991 | 56936 | 28.8% |
| oz_stable.sol | 82660 | 61801 | 25.2% |
| erc721.sol | 92738 | 64946 | 30.0% |
| erc1155.sol | 59931 | 43376 | 27.6% |
| erc20.sol | 83863 | 59724 | 28.8% |
| TimelockController | 47032 | 32709 | 30.5% |

## Identified Optimization Opportunities

### 1. Missing Bitwise Algebraic Simplifications

**Location**: `crates/newyork/src/simplify.rs` in `simplify_binary` function

**Current Status**: The `simplify_binary` function handles arithmetic operations (Add, Sub, Mul, Div, Mod) with algebraic identities, but bitwise operations (BitAnd, BitOr, BitXor) fall through to the default `_ => None` case at line 1582.

**Missing Optimizations**:
- `x & ~0 = x` (and with all-ones)
- `x & 0 = 0` (and with zero)
- `x & x = x` (and with self)
- `x | 0 = x` (or with zero)
- `x | ~0 = ~0` (or with all-ones)
- `x | x = x` (or with self)
- `x ^ 0 = x` (xor with zero)
- `x ^ x = 0` (xor with self)
- `~0 ^ x = ~x` (xor with all-ones is bitwise not)

**Evidence**: Looking at simplify.rs:1551-1582:
```rust
BinOp::Shl | BinOp::Shr | BinOp::Sar => {
    if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
        return Some(Expr::Var(rhs.id));
    }
    None
}
// ... more cases ...
_ => None,  // <-- BitAnd, BitOr, BitXor fall through here
```

**Potential Impact**: EVM bytecode frequently uses bitwise operations for boolean logic and masking. These simplifications could reduce redundant operations.

### 2. Memory Optimization Conservative at Control Flow Boundaries

**Location**: `crates/newyork/src/mem_opt.rs`

**Current Status**: The memory optimization pass clears all tracked state at control flow boundaries (if, switch, for, block). This is conservative but correct.

**Evidence**: From mem_opt.rs comments:
```rust
//! The pass is conservative at control flow joins - we clear all state when entering
//! or exiting control flow constructs (if, switch, for, block). This ensures correctness
//! but limits optimization opportunities. Future work can implement proper state merging
//! for branches.
```

**Potential Impact**: Significant load-after-store elimination opportunities may be missed in branches. This could be improved with proper state merging at control flow joins.

### 3. Function Inlining Thresholds Are Static

**Location**: `crates/newyork/src/inline.rs`

**Current Status**: Inlining thresholds are hardcoded constants:
- `ALWAYS_INLINE_SIZE_THRESHOLD = 8`
- `SINGLE_CALL_INLINE_SIZE_THRESHOLD = 40`
- `NEVER_INLINE_SIZE_THRESHOLD = 100`

**Evidence**: From inline.rs:29-42:
```rust
const ALWAYS_INLINE_SIZE_THRESHOLD: usize = 8;
const NEVER_INLINE_SIZE_THRESHOLD: usize = 100;
const SINGLE_CALL_INLINE_SIZE_THRESHOLD: usize = 40;
```

**Potential Impact**: Static thresholds may not be optimal for all contracts. A more adaptive approach could improve results. However, this is a tuning issue rather than a missing optimization.

### 4. Type Inference Limited Iteration Count

**Location**: `crates/newyork/src/lib.rs`

**Current Status**: Type inference runs 4 iterations of parameter narrowing (line 128).

**Evidence**: From lib.rs:128:
```rust
for _ in 0..4 {
    let changed = type_info.narrow_function_params(&mut ir_object);
    if !changed {
        break;
    }
    type_info.refine_demands_from_params(&ir_object);
}
```

**Potential Impact**: Some contracts may need more iterations to fully converge. This is a minor tuning issue.

### 5. Heap Analysis Limited to Static Offsets

**Location**: `crates/newyork/src/heap_opt.rs`

**Current Status**: Heap optimization analysis tracks only static (compile-time constant) memory offsets.

**Evidence**: From heap_opt.rs - the `analyze_object` function tracks memory accesses but only for known static offsets.

**Potential Impact**: Dynamic memory offsets don't benefit from heap optimization. More sophisticated alias analysis could improve this.

## Recommendations

1. **Priority 1**: Add bitwise algebraic simplifications in `simplify_binary`. This is low-hanging fruit with clear algebraic identities.

2. **Priority 2**: Improve memory optimization state merging at control flow boundaries. This requires careful handling of phi nodes but could yield significant gains.

3. **Priority 3**: Consider making inlining thresholds configurable or adaptive based on code size analysis.

## Testing Evidence

Running `RESOLC_USE_NEWYORK=1 bash oz-tests/oz.sh` shows the current optimizer achieves 25-30% codesize reduction. The above optimizations could potentially increase this further.

## Known Regression

**Test**: `tests::memory_bounds` in `revive-integration`

**Status**: FAILING with NewYork optimizer, PASSING without

**Symptom**: Memory contents differ - the NewYork optimizer is incorrectly zeroing memory at certain offsets. Expected memory: `[72, 158, 179, 62, ...]`, Actual: `[0, 0, 0, 0, ...]`

This appears to be a regression introduced by recent memory optimization changes. This needs to be investigated before further optimization work.