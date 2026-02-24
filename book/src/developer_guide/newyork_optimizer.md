# The newyork optimizer

The `newyork` crate (`crates/newyork/`) introduces an additional intermediate representation (IR) layer between Yul and LLVM IR. It enables domain-specific optimizations that neither `solc` nor LLVM can perform on their own, because they lack semantic knowledge about the cross-domain compilation from EVM to PolkaVM.

> [!NOTE]
> The newyork optimizer is experimental. It is gated behind the `RESOLC_USE_NEWYORK=1` environment variable and not yet enabled by default.

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
                    ┌──────────────────────────────────────────┐
Yul AST ──────────► │            newyork IR                    │ ──────────► LLVM IR ──► RISC-V
            from_yul│  inline → simplify → dedup → mem_opt →  │ to_llvm
                    │  fmp_prop → keccak_fold → simplify →     │
                    │  type_inference → validate                │
                    └──────────────────────────────────────────┘
```

The optimizer runs the following passes in order:

1. **Inlining** -- custom heuristics tuned for PolkaVM call overhead
2. **Simplify** (pass 1) -- constant folding, algebraic identities, copy propagation, dead code elimination
3. **Function deduplication** -- exact match, then fuzzy dedup (functions differing only in literal constants get parameterized)
4. **Memory optimization** -- load-after-store elimination, dead store elimination, keccak256 fusion
5. **Free memory pointer propagation** -- replaces `mload(0x40)` with a known constant
6. **Constant keccak256 folding** -- precomputes hashes of compile-time-constant inputs
7. **Simplify** (pass 2) -- cleans up dead code exposed by earlier passes
8. **Type inference** -- narrows 256-bit values to smaller widths where provable (iterative, up to 4 cascading refinement rounds)
9. **Validation** -- checks SSA well-formedness, type consistency, and region correctness

This pipeline runs recursively on subobjects (deployed contract code), where optimization impact is greatest.

## IR design

The newyork IR is an SSA form with structured control flow, inspired by MLIR's SCF dialect. Key design choices:

- **Explicit types with address spaces**: Every value carries a bit-width (`I1`, `I8`, `I32`, `I64`, `I160`, `I256`) and pointers carry address space information (`Heap`, `Stack`, `Storage`, `Code`). All values start as `I256` and are narrowed by type inference.
- **Pure expressions vs. effectful statements**: Expressions compute values without side effects; statements perform memory, storage, or control flow effects. This separation simplifies analysis and rewriting.
- **Semantic annotations**: Memory operations are tagged with region information (`Scratch`, `FreePointerSlot`, `Dynamic`). Storage operations carry static slot values when known at compile time.
- **Structured control flow**: `If`, `Switch`, and `For` nodes preserve the high-level structure from Yul, with explicit region arguments and yields for value flow across control edges.

## Key optimizations explained

### Type narrowing

EVM operates on 256-bit words, but most values in practice fit in 32 or 64 bits. The type inference pass performs bidirectional analysis:

- **Forward**: computes minimum width from literal values and operation semantics
- **Backward**: constrains width from use-site contexts (memory offsets, comparisons, call arguments)
- **Interprocedural**: iteratively narrows function parameter types by analyzing callers, running up to 4 refinement rounds until a fixed point is reached

This allows LLVM to emit native 32/64-bit instructions instead of software-emulated 256-bit arithmetic.

### Heap optimization

PVM doesn't provide EVM-compatible linear memory, so the compiler emulates it using a byte buffer. The heap analysis pass determines which memory regions can use native little-endian layout (skipping expensive byte-swap operations) by analyzing access patterns:

- Tracks alignment and static offset information for all memory accesses
- Propagates taintedness when addresses escape to external calls
- Currently operates as a whole-contract binary decision (native-safe or not)

### Free memory pointer range proof

The Solidity free memory pointer (`mload(0x40)`) always fits in 32 bits at the IR level. By encoding this fact via a truncate-extend pair, LLVM's range propagation eliminates overflow checks across the entire call graph. Despite only affecting a few direct sites, this produced a disproportionately large codesize reduction (see results below) due to LLVM's multiplicative propagation effect.

### Keccak256 fusion and folding

Two complementary optimizations target the common Solidity pattern of hashing values for storage slot computation:

- **Fusion**: Recognizes `mstore` + `keccak256` sequences and fuses them into dedicated IR nodes (`Keccak256Single`, `Keccak256Pair`), eliminating intermediate memory traffic.
- **Constant folding**: When all keccak256 inputs are compile-time constants, the hash is computed at compile time and replaced with a literal.

### Fuzzy function deduplication

Solidity generates many near-identical functions that differ only in literal constants (e.g., error selectors, storage slot offsets). Fuzzy deduplication identifies such groups, parameterizes the differing literals, and replaces all copies with calls to a single shared implementation.

### Outlining and shared blocks

Common exit patterns (revert with message, panic codes, zero-value returns) are outlined into shared helper functions. The `stop` instruction is deduplicated into shared return blocks. These reduce code duplication across the contract.

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
- **Per-region heap optimization**: The heap analysis infrastructure computes per-region native-safety, but codegen only uses a conservative whole-contract decision. Wiring per-region decisions would allow mixed native/emulated memory within a single contract.
- **Adaptive inlining thresholds**: Current thresholds are static constants. Profile-guided or contract-size-aware heuristics could improve decisions for diverse contract sizes.
- **Extended fuzzy deduplication**: The current pass only compares functions by structure of `Let` bindings. Extending to consider literals inside `MStore`, `Return`, `Revert`, and `Log` statements would find more deduplication opportunities.

## Module reference

| Module | Purpose |
|---|---|
| `lib.rs` | Pipeline orchestration |
| `ir.rs` | Core IR data structures (types, expressions, statements, functions, objects) |
| `from_yul.rs` | Yul AST to newyork IR translation |
| `to_llvm.rs` | newyork IR to LLVM IR codegen |
| `simplify.rs` | Constant folding, algebraic identities, copy propagation, DCE |
| `inline.rs` | Function inlining with PolkaVM-tuned heuristics |
| `type_inference.rs` | Bidirectional integer width narrowing |
| `mem_opt.rs` | Memory optimization, FMP propagation, keccak256 fusion |
| `heap_opt.rs` | Heap access pattern analysis and byte-swap elimination |
| `validate.rs` | IR well-formedness and type consistency checks |
| `printer.rs` | Human-readable IR pretty printer (for debugging) |
| `ssa.rs` | SSA construction helpers |
