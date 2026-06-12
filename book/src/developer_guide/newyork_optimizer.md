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

For per-operation detail — printed syntax, operand and result types, and more — see the [newyork IR reference](./newyork_ir.md).

## Key optimizations explained

### Type narrowing

EVM operates on 256-bit words, but most values in practice fit in 32 or 64 bits. The type inference pass performs bidirectional analysis:

- **Forward**: computes minimum width from literal values and operation semantics (e.g., `add(I64, I8)` produces `I65`, rounded up to `I128`).
- **Backward use tracking**: classifies each value's uses into 9 context categories (`MemoryOffset`, `MemoryValue`, `StorageAccess`, `Comparison`, `Arithmetic`, `FunctionArg`, `FunctionReturn`, `ExternalCall`, `General`). All categories conservatively demand the full `I256` width by default; the categorisation is what enables the interprocedural phase to selectively relax the demand for narrowed function arguments. Earlier versions narrowed directly from the use category, but that was unsound for memory offsets — `mload(2^128)` aliased to `mload(0)` because the bounds check ran on an already-truncated value (commit `ccca38df`).
- **Transparent demand propagation**: for modular-arithmetic operations (`Add`, `Sub`, `Mul`, `And`, `Or`, `Xor`), propagates narrow demands backward through operands, exploiting the property that `trunc(op(a,b), N) == op(trunc(a,N), trunc(b,N))`.
- **Interprocedural**: iteratively narrows function parameter and return types in up to four rounds, combining four narrowing strategies — body-driven parameter narrowing, caller-driven parameter narrowing, forward-based return narrowing, and demand-based return narrowing — and re-running full inference between rounds. Parameters are clamped to at least `I32` (XLEN on PolkaVM).

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

The Solidity free memory pointer (`mload(0x40)`) always fits in 32 bits — sbrk enforces `FMP < heap_size` on every store, regardless of which memory mode the contract uses. After every literal `mload(0x40)`, codegen emits a `trunc N → zext 256` chain (where N is `bits(heap_size - 1)`, e.g. 17 for the 131,072-byte default heap). The trunc-extend round-trip is a no-op semantically, but exposes the bound to LLVM's IPSCCP range analysis, which then propagates it through every `add(fmp, K)` and eliminates the trailing `safe_truncate_int_to_xlen` overflow checks at every FMP-derived offset use. Despite only affecting a single codegen site, this is the single largest contributor to the optimizer's code-size reduction.

A subtle gating issue: the byte-order mode (`InlineNative` / `ByteSwap`) and the value bound on FMP are *independent* invariants. `fmp_native_safe()` and `can_use_native(0x40)` protect against mixing little-endian writers with big-endian readers on the FMP slot, which would corrupt the stored offset; the value bound is unrelated and holds in every mode. Earlier versions of the codegen gated the load-side range proof on the byte-order checks, which suppressed the optimization for any contract with dynamic memory accesses. Decoupling the two reasonings — keeping the byte-order gate on the *store* side, dropping it from the load-side range proof — is what makes the multiplicative IPSCCP effect available to OZ-class contracts.

### Soundness traps for FMP optimizations

The FMP slot is small but easy to mis-optimize. The codebase carries several
regression tests for previously-found soundness bugs; new FMP-related changes
should be verified against them:

- **`mload_at_fmp_slot`** (`crates/integration/src/tests.rs`, fixed in
  `1fd6063c`): tests `mload(0x40)` and offsets near it (`0x21`, `0x3f`, `0x42`)
  on a contract that also performs dynamic mloads. Catches byte-order mismatches
  when one access goes native (LE) and another goes byte-swap (BE). The fix
  blocks native mode for FMP whenever `has_dynamic_accesses` is true.
- **`mload_huge_offset_traps`** (fixed in `ccca38df`): tests that
  `mload(2^128)` and `mload(2^255)` correctly trap via the gas-exhaustion
  path. Catches `UseContext::MemoryOffset` narrowing bypassing the
  `safe_truncate_int_to_xlen` overflow check at the use site —
  `mload(2^128)` aliasing to `mload(0)` and returning the zero-initialized
  scratch slot. The fix classifies `MemoryOffset` as `I256` so it doesn't
  drive narrowing; the bounds check at the use site catches out-of-range.
- **FMP i32 shortcut removal** (`dbcfc921`): an earlier optimization stored
  only 4 bytes at offset 0x40 instead of the full 32-byte EVM word, breaking
  any inline assembly using `mstore(0x40, ...)` for non-FMP purposes.
  Caused a cascade of 249/251 retester failures via allocator corruption.
  No dedicated regression test was added — the retester corpus was sufficient
  coverage — but the lesson generalizes: writes to 0x40 must store the full
  word, even when the high bits are provably zero, because the slot is part
  of the same 32-byte memory region read by other code.

When adding an optimization that touches FMP, distinguish carefully between:
the **byte-order encoding** at the slot (must be consistent between writers
and readers), the **value bound** (FMP < heap_size, always true), and the
**stored width** (must be 32 bytes for `mstore(0x40, ...)`, even though only
the low N bits are non-zero).

#### Known limitation: dynamic full-word stores to the FMP word

The `fmp_could_be_unbounded` analysis flags a static `mstore(0x40, untrusted)` and any
dynamic-offset `mstore8`, but **not** a dynamic-offset full-word `mstore`. Such a store whose
i256 offset wraps (mod 2²⁵⁶) to the FMP word `[0x40, 0x5f]` overwrites the free pointer with an
arbitrary value, which the load-side range proof would then truncate — a miscompile.

This is a deliberate, documented gap rather than a bug fix because there is no cheap sound
discriminator. A store hits the FMP word iff its offset lands in `[0x40, 0x5f]`, which is
in-bounds — `safe_truncate_int_to_xlen` only traps offsets `≥ heap_size` — and 256-bit wrap lets
any computed `add(base, k)` reach `0x40` with an adversarial operand, so the offset cannot be
proven to miss the slot from width/range information. The only sound recognizer (treat
`add(fmp, small_const)` as `≥ 0x80` by induction on FMP-boundedness) needs new FMP-derivation
dataflow, is fragile, and still misses dynamic-index array stores. Conservatively flagging *every*
dynamic full-word store (as the rare dynamic `mstore8` does, where it is free) disables the FMP
range proof for essentially every contract — measured at roughly **+9% / +30 KB** on the
OpenZeppelin corpus.

The gap is unreachable from solc output: solc's dynamic memory stores are all free-pointer-relative
(`≥ 0x80`) and never target `0x40`. Only hand-written Yul (`resolc --yul`) with an offset
engineered to equal `0x40` reaches it.

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
- **Keccak wrappers**: `__keccak256_slot_N` (one `noinline` wrapper per constant slot, internally dispatching to `__revive_keccak256_two_words`)

Additionally, common exit patterns (revert with constant length, zero-value returns) are deduplicated into shared LLVM basic blocks, saving hundreds of instruction copies in large contracts.

## Codesize results

### Integration test contracts

Reproducible with `cargo test --package revive-integration -- codesize`. The `main` column is the value committed to `crates/integration/codesize.json` on `main`; the `newyork` column is the value produced by the same test with `RESOLC_USE_NEWYORK=1` set, currently committed on this branch.

| Contract | main (bytes) | newyork (bytes) | Reduction |
|---|---|---|---|
| Baseline | 870 | 479 | −44.9% |
| Computation | 2,418 | 1,376 | −43.1% |
| DivisionArithmetics | 9,327 | 7,192 | −22.9% |
| ERC20 | 17,160 | 10,138 | −40.9% |
| Events | 1,662 | 1,279 | −23.0% |
| FibonacciIterative | 1,427 | 949 | −33.5% |
| Flipper | 2,240 | 1,123 | −49.9% |
| SHA1 | 8,009 | 6,286 | −21.5% |

### OpenZeppelin contracts

Measured by running `oz-tests/oz.sh` against real-world contracts generated with the OpenZeppelin Wizard. The numbers below are a development snapshot — there is no committed measurement file in the repo, so these may drift as the optimizer evolves; rerun the script for fresh figures.

| Contract | newyork (bytes) |
|---|---|
| oz_gov | 81,840 |
| erc721 | 52,634 |
| erc20 | 45,703 |
| oz_stable | 45,052 |
| oz_rwa | 41,581 |
| erc1155 | 33,087 |
| oz_simple_erc20 | 17,024 |
| proxy | 3,748 |
| **Total** | **320,669** |

For comparison, building the same contracts without the newyork optimizer at the equivalent snapshot produced **563,526** bytes total — a reduction of about **−43%** across the corpus.

Per-contract reductions in the integration suite range from roughly **−21%** (SHA1, where the bulk of the work is the SHA-1 inner loop and offers little to optimise) to nearly **−50%** (Flipper, where the optimiser strips away most of Solidity's dispatch and storage-access scaffolding).

## Development history and challenges

The newyork optimizer was developed over roughly three months — from early February 2026 through early May 2026 — largely through AI-assisted pair programming with Claude. The development progressed through several distinct phases:

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

## Environment variables

A small set of environment variables controls or inspects the newyork pipeline. Only `RESOLC_USE_NEWYORK` affects generated bytecode; the others are read-only inspection knobs used while debugging the optimizer.

| Variable | Effect |
|---|---|
| `RESOLC_USE_NEWYORK=1` | Routes Yul lowering through the newyork pipeline. Equivalent to passing `--newyork` on the command line; the CLI flag and this variable are OR-ed by `resolve_use_newyork` (`crates/resolc/src/lib.rs`). |
| `RESOLC_DEBUG_IR` | When set, prints the translated newyork IR for every object to stderr. Additionally writes `<output_directory>/<object>.newyork.txt` whenever the debug config carries an output directory. |
| `RESOLC_DEBUG_HEAP` | When set, appends per-object heap-analysis details — native regions/offsets, taintedness, dynamic escapes, escaping ranges — to `<output_directory>/resolc_heap_debug.log`. Requires the debug config to carry an output directory. |
| `NEWYORK_DUMP_IR` | When set, writes the IR for every translated object to `/tmp/newyork_ir_<object>.txt` from inside `translate_yul_object` (`crates/newyork/src/lib.rs`). Independent of `RESOLC_DEBUG_IR` — fires before codegen and needs no output directory. |
| `RESOLC_DEBUG_BLOB` | Test harness only. Dumps the compiled PVM blob to `/tmp/debug_blob_<contract>.pvm` and the LLVM IR debug directory to `/tmp/debug_llvm_newyork` or `/tmp/debug_llvm_yul`. Used by `crates/resolc/src/test_utils.rs` when comparing newyork against the Yul path. |

All of these gate on presence/value at the start of compilation; flipping them mid-run has no effect.

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
