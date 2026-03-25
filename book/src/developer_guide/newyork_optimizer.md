# The newyork optimizer

The `newyork` crate (`crates/newyork/`) introduces an additional intermediate representation (IR) layer between Yul and LLVM IR. It enables domain-specific optimizations that neither `solc` nor LLVM can perform on their own, because they lack semantic knowledge about the cross-domain compilation from EVM to PolkaVM.

> [!NOTE]
> The newyork optimizer is experimental. It is gated behind the `RESOLC_USE_NEWYORK=1` environment variable (for standard JSON mode) or the `--newyork` CLI flag, and not yet enabled by default.

## Motivation

The EVM and PolkaVM are fundamentally different machines:

| Property | EVM | PolkaVM (RISC-V) |
|---|---|---|
| Word size | 256-bit | 64-bit |
| Endianness | Big-endian | Little-endian |
| Architecture | Stack machine | Register-based |
| Memory model | Linear with free pointer convention | Flat address space |

`solc` optimizes Yul IR for EVM gas costs on a 256-bit big-endian stack machine. LLVM, on the other hand, operates at too low a level to understand EVM memory semantics or Solidity patterns. By the time Yul reaches LLVM IR, the high-level intent is lost.

The newyork IR sits between these two worlds and recovers enough semantic information to make optimization decisions that neither compiler can make alone.

## Pipeline overview

```text
                    ┌──────────────────────────────────────────────────┐
Yul AST ──────────► │                  newyork IR                      │ ──► LLVM IR ──► RISC-V
            from_yul│                                                  │ to_llvm
                    │  1. inline                                       │
                    │  2. simplify (pass 1)                            │
                    │  3. dedup (exact + fuzzy)                        │
                    │  4. mem_opt + fmp_prop + keccak_fold             │
                    │  5. simplify (pass 2)                            │
                    │  6. compound_outlining + guard_narrow            │
                    │  7. simplify (pass 3)                            │
                    │  8. dedup (exact + fuzzy, pass 2)                │
                    │  ── recursive on subobjects ──                   │
                    │  9. heap_opt (analysis)                          │
                    │ 10. type_inference (4 iterative rounds)          │
                    │ 11. validate                                     │
                    └──────────────────────────────────────────────────┘
```

The optimizer runs the following passes in order:

1. **Inlining** -- custom heuristics tuned for PolkaVM call overhead, with Tarjan SCC-based recursion detection and quadratic leave-overhead modeling.
2. **Simplify** (pass 1) -- constant folding, algebraic identities, strength reduction (`mul` by power-of-2 to `shl`), copy propagation, dead code elimination, environment read CSE (`callvalue`, `caller`, `origin`, etc.), and revert pattern outlining (panic selectors, custom error selectors).
3. **Function deduplication** -- exact structural match, then fuzzy dedup (functions differing only in literal constants are parameterized and merged, up to 4 differing positions).
4. **Memory optimization** -- load-after-store elimination, keccak256 fusion (`mstore` + `keccak256` sequences into `Keccak256Single`/`Keccak256Pair` nodes), free memory pointer propagation (replaces `mload(0x40)` with a known constant), and constant keccak256 folding (precomputes hashes of compile-time-constant inputs).
5. **Simplify** (pass 2) -- cleans up dead code and new constant expressions exposed by memory optimization and keccak folding.
6. **Compound outlining** -- detects `keccak256_pair` + `sload`/`sstore` sequences and fuses them into `MappingSLoad`/`MappingSStore` IR nodes, eliminating intermediate hash values. **Guard narrowing** -- detects `if gt(val, MASK) { revert }` and `iszero(eq(val, and(val, MASK)))` patterns and inserts AND-mask narrowing, giving type inference proof that values fit in fewer bits.
7. **Simplify** (pass 3) -- propagates opportunities created by compound outlining and guard narrowing.
8. **Function deduplication** (pass 2) -- catches new duplicates exposed by guard narrowing and compound outlining canonicalization.
9. **Heap analysis** -- analyzes memory access patterns (alignment, static offsets, taintedness, escaping regions) to determine which accesses can use native little-endian layout, skipping byte-swap operations. Uses GCD-based alignment propagation and per-region taint tracking.
10. **Type inference** -- narrows 256-bit values to smaller widths (`I1`, `I8`, `I32`, `I64`, `I128`, `I160`) where provable. Runs iteratively for up to 4 cascading refinement rounds, combining forward min-width propagation, backward use-context demands, transparent-operation demand propagation, and interprocedural parameter/return narrowing.
11. **Validation** -- checks SSA well-formedness (use-before-def, multiple definitions), yield count consistency, and function reference correctness.

Steps 1-8 run recursively on subobjects (deployed contract code), where optimization impact is greatest. Steps 9-11 run on the full object tree.

## IR design

The newyork IR is an SSA form with structured control flow, inspired by MLIR's SCF dialect. Key design choices:

- **Explicit types with address spaces**: Every value carries a bit-width (`I1`, `I8`, `I32`, `I64`, `I128`, `I160`, `I256`) and pointers carry address space information (`Heap`, `Stack`, `Storage`, `Code`). All values start as `I256` and are narrowed by type inference.
- **Pure expressions vs. effectful statements**: Expressions compute values without side effects; statements perform memory, storage, or control flow effects. This separation simplifies analysis and rewriting.
- **Semantic annotations**: Memory operations are tagged with region information (`Scratch`, `FreePointerSlot`, `Dynamic`). Storage operations carry static slot values when known at compile time.
- **Structured control flow**: `If`, `Switch`, and `For` nodes preserve the high-level structure from Yul, with explicit region arguments and yields for value flow across control edges.

## Key optimizations explained

### Type narrowing

EVM operates on 256-bit words, but most values in practice fit in 32 or 64 bits. The type inference pass performs bidirectional analysis:

- **Forward**: computes minimum width from literal values and operation semantics (e.g., `add(I64, I8)` produces `I65`, rounded up to `I128`)
- **Backward**: constrains width from use-site contexts via 9 context types (`MemoryOffset` demands `I64`, `StorageAccess` demands `I256`, etc.)
- **Transparent demand propagation**: for modular-arithmetic operations (`Add`, `Sub`, `Mul`, `And`, `Or`, `Xor`), propagates narrow demands backward through operands, exploiting the property that `trunc(op(a,b), N) == op(trunc(a,N), trunc(b,N))`
- **Interprocedural**: iteratively narrows function parameter and return types by analyzing callers and call sites, running up to 4 refinement rounds until a fixed point is reached. Parameters are clamped to at least `I32` (XLEN on PolkaVM).

This allows LLVM to emit native 32/64-bit instructions instead of software-emulated 256-bit arithmetic, and eliminates expensive multi-instruction comparison sequences (16-20 RISC-V instructions for i256 comparisons reduced to 1-2 for i64).

### Guard narrowing

Solidity emits runtime guards that prove values fit in narrow ranges (e.g., address validation via `if gt(val, 2^160-1) { revert }`). The guard narrowing pass detects these patterns and inserts explicit AND-mask narrowing after the guard. This gives downstream type inference proof that the value fits in fewer bits, enabling cascading narrowing of comparisons, arithmetic, and memory operations that use the guarded value.

Two pattern families are recognized:

- **GT-based guards**: `if gt(val, MASK) { <terminates> }` where MASK is a boundary value like `2^N - 1`
- **EQ-based guards**: `iszero(eq(val, and(val, MASK)))` patterns common in Solidity's address validation

### Heap optimization

PVM doesn't provide EVM-compatible linear memory, so the compiler emulates it using a byte buffer with byte-swap operations for big-endian compatibility. The heap analysis pass determines which memory accesses can use native little-endian layout by analyzing access patterns:

- Tracks alignment and static offset information for all memory accesses using GCD-based propagation
- Propagates taintedness when addresses escape to external calls, are written by external sources (`codecopy`, `calldatacopy`), or use unaligned access patterns
- Tracks variable-accessed offsets to prevent mode mismatches between native and byte-swap accesses to the same location
- Handles loop-carried variables conservatively (marked as non-literal to prevent false constant propagation)

The codegen backend supports four memory access modes: `AllNative` (all accesses skip byte-swap), `InlineNative` (constant-offset accesses use native layout), `InlineByteSwap` (constant-offset accesses use inline byte-swap), and `ByteSwap` (standard byte-swap through helper functions).

### Free memory pointer range proof

The Solidity free memory pointer (`mload(0x40)`) always fits in 32 bits at the IR level. By encoding this fact via a truncate-extend pair, LLVM's range propagation eliminates overflow checks across the entire call graph. Despite only affecting a few direct sites, this produced a disproportionately large codesize reduction (see results below) due to LLVM's multiplicative propagation effect.

### Keccak256 fusion and folding

Two complementary optimizations target the common Solidity pattern of hashing values for storage slot computation:

- **Fusion**: Recognizes `mstore` + `keccak256` sequences and fuses them into dedicated IR nodes (`Keccak256Single`, `Keccak256Pair`), eliminating intermediate memory traffic.
- **Constant folding**: When all keccak256 inputs are compile-time constants, the hash is computed at compile time and replaced with a literal.

### Compound outlining (mapping access)

Solidity mapping accesses follow a predictable pattern: hash a key with a storage slot, then load/store the result. The compound outlining pass detects `keccak256_pair(key, slot)` followed by `sload`/`sstore` and fuses them into `MappingSLoad`/`MappingSStore` IR nodes. These are lowered to outlined helper functions (`__revive_mapping_sload`, `__revive_mapping_sstore`) that combine the hash computation with the storage operation, eliminating intermediate values and redundant byte-swaps.

### Fuzzy function deduplication

Solidity generates many near-identical functions that differ only in literal constants (e.g., error selectors, storage slot offsets). Fuzzy deduplication identifies such groups, parameterizes the differing literals (up to 4 positions), and replaces all copies with calls to a single shared implementation.

### Revert pattern outlining

The simplify pass detects common revert patterns and replaces them with compact IR nodes:

- **Panic reverts**: Solidity `Panic(uint256)` sequences (selector `0x4e487b71` + encoded panic code) are collapsed into `PanicRevert { code }` nodes, which are lowered to shared helper functions.
- **Custom error reverts**: ABI-encoded custom error reverts with known selectors are collapsed into `CustomErrorRevert { selector, args }` nodes.

These patterns appear dozens of times in typical contracts, and outlining them into shared blocks eliminates significant code duplication.

### Outlined helper functions

The LLVM codegen backend generates approximately 15 types of outlined helper functions for common operations:

- **Storage**: `__revive_sload_word`, `__revive_sstore_word` (handle byte-swap internally)
- **Mapping**: `__revive_mapping_sload`, `__revive_mapping_sstore` (keccak256 + storage in one call)
- **Callvalue**: `__revive_callvalue`, `__revive_callvalue_nonzero` (boolean optimization for non-payable checks)
- **Calldataload**: `__revive_calldataload` (outlined when >= 20 call sites)
- **Memory**: `__revive_store_bswap`, `__revive_exit_checked`, `__revive_return_word`
- **Errors**: `__revive_error_string_revert_N`, `__revive_custom_error_N` (per data-word count)
- **Keccak wrappers**: `__revive_keccak256_slot_wrapper_*` (per constant slot)

Additionally, common exit patterns (revert with constant length, zero-value returns) are deduplicated into shared LLVM basic blocks, saving hundreds of instruction copies in large contracts.

## Codesize results

### Integration test contracts

Measured against the main branch baseline:

| Contract | main (bytes) | newyork (bytes) | Reduction |
|---|---|---|---|
| Baseline | 870 | 649 | -25.4% |
| Computation | 2,418 | 1,591 | -34.2% |
| DivisionArithmetics | 9,327 | 7,681 | -17.6% |
| ERC20 | 17,160 | 11,849 | -30.9% |
| Events | 1,662 | 1,434 | -13.7% |
| FibonacciIterative | 1,427 | 1,201 | -15.8% |
| Flipper | 2,240 | 1,536 | -31.4% |
| SHA1 | 8,009 | 5,958 | -25.6% |

### OpenZeppelin contracts

Measured on real-world contracts generated with the OpenZeppelin Wizard:

| Contract | Baseline (bytes) | newyork (bytes) | Reduction |
|---|---|---|---|
| Governor (oz_gov.sol) | 147,712 | 105,448 | -28.6% |
| RWA Token (oz_rwa.sol) | 79,991 | 56,936 | -28.8% |
| Stablecoin (oz_stable.sol) | 82,660 | 61,801 | -25.2% |
| ERC-721 | 92,738 | 64,946 | -30.0% |
| ERC-1155 | 59,931 | 43,376 | -27.6% |
| ERC-20 | 83,863 | 59,724 | -28.8% |
| TimelockController | 47,032 | 32,709 | -30.5% |

The optimizer consistently achieves **25-34% codesize reduction** across both small test contracts and large real-world contracts.

## Development history and challenges

The newyork optimizer was developed over approximately three weeks in February 2026, largely through AI-assisted pair programming with Claude. The development progressed through several distinct phases:

**Phase 1 -- Initial scaffolding**: The first draft established the core IR data structures, Yul-to-IR translation, and LLVM codegen. Early commits focused on getting a correct round-trip through the new pipeline.

**Phase 2 -- Optimization passes**: Once the baseline was stable, optimization passes were added iteratively: inlining, simplification, memory optimization, function deduplication, keccak256 fusion, and type inference. Each pass was validated against differential tests comparing EVM and PVM execution.

**Phase 3 -- Soundness hardening**: Several type inference and narrowing approaches turned out to be unsound and had to be reworked:
- An early type inference approach caused namespace collisions across subobjects and was scoped per-object.
- Caller-based parameter narrowing was polluted by overly aggressive inference and replaced with body-based structural analysis.
- Backward demand-driven narrowing required multiple iterations to become provably safe.

**Phase 4 -- Measuring and tuning**: Systematic measurement of OpenZeppelin contracts revealed which optimizations had the most impact and which approaches regressed performance.

### Approaches that did not work

| Approach | Outcome |
|---|---|
| Storage bswap decomposition (4x bswap.i64) | Regressed: LLVM handles bswap.i256 better natively |
| NoInline on `__revive_int_truncate` | +62% regression: PolkaVM call overhead exceeds inline cost |
| Native FMP memory (inline sbrk) | Mixed: small contracts improved, large ones regressed from sbrk bloat |
| Shared overflow trap block | Mixed: prevented LLVM from eliminating individual dead overflow checks |

These results highlight a recurring theme: interacting well with LLVM's own optimization passes is critical. Optimizations at the IR level can inadvertently inhibit LLVM's downstream passes, sometimes causing surprising regressions.

## Known limitations and future work

The following opportunities have been identified but are not yet implemented:

- **Bitwise algebraic simplifications**: `BitAnd`, `BitOr`, `BitXor` identity patterns fall through without simplification.
- **Cross-control-flow memory optimization**: Memory state is conservatively cleared at `if`/`switch`/`for` boundaries. Preserving state across simple branches would enable more load-after-store eliminations.
- **Adaptive inlining thresholds**: Current thresholds are static constants. Profile-guided or contract-size-aware heuristics could improve decisions for diverse contract sizes.
- **Extended fuzzy deduplication**: The current pass only compares functions by structure of `Let` bindings. Extending to consider literals inside `MStore`, `Return`, `Revert`, and `Log` statements would find more deduplication opportunities.
- **Type checking in validation**: The validator checks SSA well-formedness and structural correctness, but does not yet verify type consistency of operations (the `TypeMismatch` error variant exists but is not yet wired).
- **Loop variable narrowing**: Loop-carried variables are conservatively widened to `I256`. Reaching a fixed-point across loop iterations could allow narrower types for simple counters.

## Module reference

| Module | Purpose |
|---|---|
| `lib.rs` | Pipeline orchestration and pass sequencing |
| `ir.rs` | Core IR data structures (types, expressions, statements, functions, objects) |
| `from_yul.rs` | Yul AST to newyork IR translation (two-pass with forward reference support) |
| `to_llvm.rs` | newyork IR to LLVM IR codegen with outlined helpers and narrowing |
| `simplify.rs` | Constant folding, algebraic identities, strength reduction, copy propagation, DCE, environment read CSE, revert outlining, callvalue hoisting, function deduplication (exact and fuzzy), constant keccak folding |
| `inline.rs` | Function inlining with PolkaVM-tuned heuristics (Tarjan SCC, leave elimination) |
| `type_inference.rs` | Bidirectional integer width narrowing with transparent demand propagation |
| `mem_opt.rs` | Load-after-store elimination, keccak256 fusion, FMP propagation |
| `heap_opt.rs` | Heap access pattern analysis, alignment tracking, byte-swap elimination |
| `compound_outlining.rs` | Mapping access pattern detection and fusion (`keccak256_pair` + `sload`/`sstore`) |
| `guard_narrow.rs` | Guard pattern detection and AND-mask narrowing insertion |
| `validate.rs` | IR well-formedness checks (SSA, yields, function references) |
| `printer.rs` | Human-readable IR pretty printer with configurable output |
| `ssa.rs` | SSA construction helpers (scope management, phi-node merging) |
