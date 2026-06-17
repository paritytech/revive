# The newyork optimizer

The `newyork` crate (`crates/newyork/`) introduces an additional intermediate representation (IR) layer between Yul and LLVM IR. It enables domain-specific optimizations that neither `solc` nor LLVM can perform on their own, because they lack semantic knowledge about the cross-domain compilation from EVM to PolkaVM.

> [!NOTE]
> The newyork optimizer is experimental. It is gated behind the `--newyork` CLI flag or the `settings.polkavm.newyork` field in standard JSON input, and not yet enabled by default.

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
                    │  6. mapping_access_outlining + guard_narrow      │
                    │  7. simplify (pass 3)                            │
                    │  8. dedup (exact + fuzzy, pass 2)                │
                    │  ── recursive on subobjects ──                   │
                    │  9. type_inference (iterative narrowing)         │
                    │ 10. late inline loop: inline, simplify, outline, │
                    │     guard-narrow, simplify, dedup, narrow        │
                    │ 11. heap_opt (analysis)                          │
                    │ 12. validate                                     │
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
9. **Type inference** -- narrows 256-bit values to smaller widths (`I1`, `I8`, `I32`, `I64`, `I128`, `I160`) where provable. Runs iteratively for up to 4 cascading refinement rounds, combining forward min-width propagation, backward use-context demands, transparent-operation demand propagation, and interprocedural parameter/return narrowing.
10. **Late inline loop** -- now that narrowing and simplification have shrunk wrapper functions below the inline thresholds, re-runs inlining, simplification, mapping access outlining, guard narrowing, deduplication, and type inference to collect the residual opportunities.
11. **Heap analysis** -- analyzes memory access patterns (alignment, static offsets, taintedness, escaping regions) to determine which accesses can use native little-endian layout, skipping byte-swap operations. Uses GCD-based alignment propagation and per-region taint tracking.
12. **Validation** -- checks SSA well-formedness (use-before-def, multiple definitions), yield count consistency, and function reference correctness.

Steps 1-8 run recursively on subobjects (deployed contract code), where optimization impact is greatest. Steps 9-12 run on the full object tree.

## IR design

The newyork IR is an SSA form with structured control flow, inspired by [MLIR's SCF dialect](https://mlir.llvm.org/docs/Dialects/SCFDialect/). Key design choices:

- **Explicit types with address spaces**: Every value carries a bit-width (`I1`, `I8`, `I32`, `I64`, `I128`, `I160`, `I256`) and pointers carry address space information (`Heap`, `Stack`, `Storage`, `Code`). All values start as `I256` and are narrowed by type inference.
- **Pure expressions vs. effectful statements**: Expressions compute values without side effects; statements perform memory, storage, or control flow effects. This separation simplifies analysis and rewriting.
- **Semantic annotations**: Memory operations are tagged with region information (`Scratch`, `FreePointerSlot`, `Dynamic`). Storage operations carry static slot values when known at compile time.
- **Structured control flow**: `If`, `Switch`, and `For` nodes preserve the high-level structure from Yul, with explicit region arguments and yields for value flow across control edges.

For per-operation detail — printed syntax, operand and result types, and more — see the [newyork IR reference](./newyork_ir.md).

## Key optimizations explained

### Type narrowing

EVM operates on 256-bit words, but most values in practice fit in 32 or 64 bits. The type inference pass performs bidirectional analysis:

- **Forward**: computes minimum width from literal values and operation semantics (e.g., `add(I64, I8)` produces `I65`, rounded up to `I128`).
- **Backward use tracking**: classifies each value's uses into 9 context categories (`MemoryOffset`, `MemoryValue`, `StorageAccess`, `Comparison`, `Arithmetic`, `FunctionArg`, `FunctionReturn`, `ExternalCall`, `General`). All categories conservatively demand the full `I256` width by default; the categorization is what enables the interprocedural phase to selectively relax the demand for narrowed function arguments. Earlier versions narrowed directly from the use category, but that was unsound for memory offsets — `mload(2^128)` aliased to `mload(0)` because the bounds check ran on an already-truncated value (commit `ccca38df`).
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

#### Known limitation: constant-folding drops the fused-keccak scratch write-back

The fused `Keccak256Pair`/`Keccak256Single` helpers write their inputs back to scratch memory
(`[0, 0x40)` / `[0, 0x20)`), and fusion *dead-eliminates the original `mstore`s* because that
write-back reproduces them. Constant-folding the fused node to a literal removes the helper, so the
scratch is left unwritten — a later `mload` from `[0, 0x40)` that the optimizer cannot forward
(across a region/call boundary) would read stale memory.

This gap is deliberately left open: it is solc-unreachable (solc treats scratch as volatile and never
re-reads it as data after a keccak), and every sound fix is a code-size regression because the dropped
write-back means the current output is already short the stores (disabling the fold falls back to the
runtime keccak helper, +0.78% on the OZ corpus, with no later mem_opt pass to clean re-emitted
writes). Only hand-written Yul that reads scratch after a constant-operand keccak across a boundary
can observe it.

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

Reproducible with `cargo test --package revive-integration -- codesize` for the *Via Yul IR* column (`crates/integration/codesize.json`) and `cargo test --package revive-integration --features newyork -- codesize` for the *Via newyork IR* column (`crates/integration/codesize_newyork.json`).

| Contract | Via Yul IR (bytes) | Via newyork IR (bytes) | Reduction |
|---|---|---|---|
| Baseline | 838 | 493 | −41.2% |
| Computation | 2,368 | 1,217 | −48.6% |
| DivisionArithmetics | 11,444 | 7,370 | −35.6% |
| ERC20 | 18,057 | 8,726 | −51.7% |
| Events | 1,614 | 909 | −43.7% |
| FibonacciIterative | 1,373 | 969 | −29.4% |
| Flipper | 2,205 | 1,058 | −52.0% |
| SHA1 | 7,830 | 6,264 | −20.0% |

### OpenZeppelin contracts

Measured against real-world contracts generated with the OpenZeppelin Wizard. The numbers below are a development snapshot.

| Contract | Via newyork IR (bytes) |
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

Per-contract reductions in the integration suite range from roughly **−20%** (SHA1, where the bulk of the work is the SHA-1 inner loop and offers little to optimize) to about **−52%** (Flipper, where the optimizer strips away most of Solidity's dispatch and storage-access scaffolding).

## Development history and challenges

The first version of the newyork optimizer was authored collaboratively and reviewed extensively by the `revive` maintainers, as well as Claude Opus, Claude Fable, Qwen, Minimax and Deepseek LLMs, over a span of many months — from early February 2026 through mid-June 2026.

The development progressed through several distinct phases:

**Phase 1 -- Initial scaffolding**: The first draft established the core IR data structures, Yul-to-newyork-IR translation, and LLVM codegen. Early commits focused on getting a correct round-trip through the new pipeline.

**Phase 2 -- Optimization passes**: Once the baseline was stable, optimization passes were added iteratively: inlining, simplification, memory optimization, function deduplication, keccak256 fusion, and type inference. Each pass was validated against differential tests comparing EVM and PVM execution.

**Phase 3 -- Soundness hardening**: Several type inference and narrowing approaches turned out to be unsound and had to be reworked:
- An early type inference approach caused namespace collisions across subobjects and was scoped per-object.
- Caller-based parameter narrowing was polluted by overly aggressive inference and replaced with body-based structural analysis.
- Backward demand-driven narrowing required multiple iterations to become provably safe.

**Phase 4 -- Measuring and tuning**: Systematic measurement of OpenZeppelin contracts revealed which optimizations had the most impact and which approaches regressed performance.

Throughout development the optimizer was validated against the existing integration and differential test suites (containing over 30,000 test cases), which run each contract on both EVM and PVM and assert identical state changes.

The newyork compiler pipeline introduced no new regressions over these test suites. This was achieved by careful manual reviews and many LLM bughunt loops. Additionally, a final security review by Anthropic's Fable 5 LLM found no remaining soundness issues. As with any new compiler feature, it should still be treated as experimental as of now.

### Approaches that did not work

| Approach | Outcome |
|---|---|
| Storage bswap decomposition (4x bswap.i64) | Regressed: LLVM handles bswap.i256 better natively |
| NoInline on `__revive_int_truncate` | +62% regression: PolkaVM call overhead exceeds inline cost |
| Native FMP memory (inline sbrk) | Mixed: small contracts improved, large ones regressed from sbrk bloat |
| Shared overflow trap block | Mixed: prevented LLVM from eliminating individual dead overflow checks |
| Aggressive IR-level single-call inlining | Regressed large contracts (ERC20 +6.1%): merged bodies become monolithic functions LLVM can't optimize, so large functions are deferred to LLVM's inliner instead |
| Type-inference narrowing of `mload(0x40)` to I32 | Regressed small contracts (+252 bytes): conflicts with the codegen FMP range proof; the bound is exposed via a trunc→zext pair instead |
| Full simplifier re-run after `mem_opt` | Mixed: helped small ERC20 (−293 bytes) but regressed the OZ stablecoin (+72 bytes); replaced by a targeted keccak-only fold |

These results highlight a recurring theme: interacting well with LLVM's own optimization passes is critical. Optimizations at the IR level can inadvertently inhibit LLVM's downstream passes, sometimes causing surprising regressions.

## Known limitations and future work

The following opportunities have been identified but are not yet implemented:

- **Memory optimization across loop boundaries**: Tracked memory state is cleared around `for` loop condition, body, and post blocks, so load-after-store eliminations do not carry across loop iterations. Preserving loop-invariant state would recover more eliminations.
- **Adaptive inlining thresholds**: Current thresholds are static constants. Profile-guided or contract-size-aware heuristics could improve decisions for diverse contract sizes.
- **Extended fuzzy deduplication**: The current pass only parameterizes literals in `Let` bindings and `SStore` slots. Extending it to consider literals inside `MStore`, `Return`, `Revert`, and `Log` statements would find more deduplication opportunities.
- **Type checking in validation**: The validator checks SSA well-formedness and structure, but not operation type consistency. Type discipline is maintained by construction (type inference and codegen), with LLVM's IR verifier as the backstop.
- **Loop variable narrowing**: Loop-carried variables are conservatively widened to `I256`. Reaching a fixed-point across loop iterations could allow narrower types for simple counters.
- **Functions with `leave` inside a `for` loop are not inlined**: the IR-level inliner defers such functions to LLVM's inliner, so they miss the interprocedural constant propagation and width narrowing the IR-level pass provides.

## Debug output

Passing `--debug-output-dir <path>` makes the newyork pipeline write IR and analysis artifacts for each compiled contract into that directory. The dumps are produced automatically whenever the directory is set.

| File | Content |
|---|---|
| `<contract-stem>.newyork` | Final optimized IR, annotated with the inferred type widths |
| `<contract-stem>.snapshot.newyork` | IR snapshot taken before the late passes (only when captured during translation) |
| `<contract-stem>.heap.newyork` | Heap analysis summary (native regions/offsets, taintedness, escapes, dynamic accesses) |
| `<contract-stem>.mem.newyork` | Memory optimization counters (loads/stores eliminated, keccak fusions, FMP loads eliminated) |

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
| `mapping_access_outlining.rs` | Mapping access pattern detection and fusion (`keccak256_pair` + `sload`/`sstore`) |
| `guard_narrow.rs` | Guard pattern detection and AND-mask narrowing insertion |
| `validate.rs` | IR well-formedness checks (SSA, yields, function references) |
| `printer.rs` | Human-readable IR pretty printer with configurable output |
| `ssa.rs` | SSA construction helpers (scope management, phi-node merging) |
