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

### 6. Unused Phase 2 Type Conversion Infrastructure

**Location**: `crates/newyork/src/to_llvm.rs:815-846`

**Current Status**: `convert_to_inferred_type` function exists but is marked `#[allow(dead_code)]`. This function is designed to convert values to inferred types for storage, but is not being used.

**Evidence**:
```rust
#[allow(dead_code)]
fn convert_to_inferred_type(
    &self,
    context: &PolkaVMContext<'ctx>,
    value: IntValue<'ctx>,
    target_id: ValueId,
    name: &str,
) -> Result<IntValue<'ctx>> {
```

**Potential Impact**: This infrastructure could be used to generate narrower store operations when the stored value is known to be narrower than 256 bits.

### 7. Memory State Merge Functions Not Used

**Location**: `crates/newyork/src/mem_opt.rs:1108-1140`

**Current Status**: `merge_states` and related functions for merging memory states at control flow joins are implemented but not called. Currently the pass clears all state at control flow boundaries.

**Evidence**: The comments say "not yet used - we currently clear state at control flow boundaries for safety"

**Potential Impact**: Enabling these would allow load-after-store optimization to work across branches, significantly improving optimization in typical Solidity code with require checks.

### 8. No Unary Expression Algebraic Simplifications

**Location**: `crates/newyork/src/simplify.rs:777-791`

**Current Status**: The `simplify_expr` for Unary only does constant folding, not algebraic identities. There are no simplifications like:
- `not(not(x)) = x` (double negation)
- `iszero(iszero(x)) = x`
- `clz(0) = 256` (already handled)

**Potential Impact**: Could eliminate redundant unary operations in generated code.

### 9. No Ternary Expression Simplifications Beyond Constant Folding

**Location**: `crates/newyork/src/simplify.rs:794-812`

**Current Status**: Only constant folding is done for ternary expressions. Missing optimizations:
- `c ? x : x = x` (both branches same)
- `c ? true : false = c`
- `c ? false : true = not(c)`

**Potential Impact**: Could eliminate redundant select operations.

### 10. No Short-Circuit Evaluation Optimization

**Location**: `crates/newyork/src/simplify.rs`

**Current Status**: Logical And/Or operations (`and(x, y)` and `or(x, y)`) don't use short-circuit evaluation. Both operands are always evaluated.

**Potential Impact**: For expressions like `and(lt(x, 10), gt(x, 0))`, if first condition is false, second doesn't need to be evaluated. This could save gas in some cases.

### 11. Division by Constant Not Optimized to Multiplication

**Location**: `crates/newyork/src/simplify.rs:1384-1583`

**Current Status**: Division by constant literals could be optimized to reciprocal multiplication for better performance, though this is more of a perf optimization than codesize.

**Potential Impact**: Limited codesize impact but could improve runtime.

### 12. No Loop Unrolling

**Location**: `crates/newyork/src/` - not implemented

**Current Status**: The optimizer doesn't unroll loops. Many Solidity contracts have small fixed-iteration loops that could be unrolled.

**Evidence**: Looking at the LLVM IR shows many loop structures preserved from Yul.

**Potential Impact**: Could eliminate branch overhead for small loops.

### 13. No Common Subexpression Elimination Across Basic Blocks

**Location**: `crates/newyork/src/simplify.rs`

**Current Status**: CSE is limited to the current statement scope. Cross-basic-block CSE is not implemented.

**Potential Impact**: Could eliminate redundant computations in control flow heavy code.

### 14. Switch Lowering Not Optimized to Jumptables

**Location**: `crates/newyork/src/to_llvm.rs` or `crates/newyork/src/inline.rs`

**Current Status**: The switch statement in Yul might not be lowered to efficient jumptables in LLVM.

**Evidence**: Only 1 switch statement in ERC20 optimized LLVM, suggesting they're being lowered to if-else chains.

**Potential Impact**: Jumptables are more efficient for large dispatch tables.

### 15. No Deduplication of External Call Wrappers

**Location**: `crates/newyork/src/simplify.rs` - function deduplication

**Current Status**: Function deduplication works for internal functions but external call wrappers (which often differ only in parameters) may not be merged.

**Evidence**: Multiple `call_evm` wrapper patterns in the LLVM output.

**Potential Impact**: Could reduce code duplication in contracts with many external calls.

### 16. Stack Variable Coalescing Not Performed

**Location**: `crates/newyork/src/ssa.rs` or later stages

**Current Status**: SSA form doesn't optimize away unused intermediate values that remain on stack.

**Potential Impact**: Minor - LLVM generally handles this.

### 17. No Zero-Initialization Optimization

**Location**: `crates/newyork/src/simplify.rs`

**Current Status**: When memory is zeroed and then written, the zeroing could be eliminated if the write covers the same region.

**Evidence**: In the IR, explicit zeroing followed by stores to same region.

**Potential Impact**: Could reduce initialization overhead in ABI encoding functions.

### 18. Copy Propagation Not Using Type Information

**Location**: `crates/newyork/src/simplify.rs`

**Current Status**: Copy propagation happens on the raw IR, not using type constraints to know that a value is only used in narrow context.

**Evidence**: Values that flow to memory offsets could be narrowed earlier if copy propagation used type info.

**Potential Impact**: Could work with type inference to narrow values earlier.

### 19. No Peephole Optimizations

**Location**: Not implemented in newyork

**Current Status**: There are no explicit peephole optimizations (local pattern rewrites) in newyork - relies entirely on algebraic simplifications.

**Potential Impact**: Could add patterns like `add(mul(x, 2), y) -> mul(x, 2)` (though this is already handled by algebraic simplifications).

### 20. String Literal Deduplication Not Aggressive

**Location**: `crates/newyork/src/from_yul.rs`

**Current Status**: String constants (for error messages, event signatures) may not be fully deduplicated across the entire object tree.

**Evidence**: Multiple instances of same string constants in LLVM data section.

**Potential Impact**: Could reduce data section size.

### 21. No Return Data Size Optimization

**Location**: Various

**Current Status**: `returndatasize()` calls might not be eliminated when the return buffer size is known.

**Evidence**: `returndatasize` appears in LLVM output even when return sizes are static.

**Potential Impact**: Could eliminate redundant size checks.

### 22. Gas Stipend Optimization Not Applied

**Location**: `to_llvm.rs`

**Current Status**: External calls with known gas requirements could have stipend optimized.

**Potential Impact**: Minor codesize impact but could improve call efficiency.

### 23. Event Topic Hashing Not Precomputed

**Location**: `from_yul.rs` or `simplify.rs`

**Current Status**: Event topic hashes (keccak256 of event signatures) are computed at runtime.

**Evidence**: `keccak256` calls for event signatures visible in Yul.

**Potential Impact**: Computing these at compile time would save runtime gas and potentially code size.

### 24. Immutable Variable Loading Not Optimized

**Location**: Various

**Current Status**: Immutable variables are loaded from storage on every access. Could be cached in memory for the duration of a transaction.

**Evidence**: Multiple sload operations for same immutable in LLVM output.

**Potential Impact**: Significant gas savings for contracts with many immutable reads.

### 25. Storage Reading Could Use Calldata When Available

**Location**: `to_llvm.rs`

**Current Status**: For view functions that only read storage, the runtime could potentially use a more efficient path.

**Potential Impact**: Minor.

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