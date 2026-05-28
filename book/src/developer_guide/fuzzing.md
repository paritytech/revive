# Differential fuzzing

`revive` ships three fuzzers that all compare the same logical contract
execution between resolc's PVM lowering and solc's EVM lowering. Any
byte-level mismatch on revert flags or return data is treated as a
backend bug. Two of the three are pure-random; the third runs
libFuzzer with SanitizerCoverage feedback over every Rust crate in
resolc's dep graph, so the mutation engine learns from edge coverage
in the parser, IR lowering, and pallet-revive simulation.

> [!TIP]
>
> All three fuzzers shell out to `solc` and geth's `evm`. Both must be on
> `$PATH`. See the [Testing strategy](./testing.md) chapter for the geth
> EVM-tool installation snippet.

## Running the fuzzers

There are three entry points. All share one generator, one
divergence-comparator, and one observation shape — they differ only
in how they sample inputs and how findings are surfaced.

### Pure-random Solidity (`revive-fuzz`)

```bash
# 100 iterations on a fresh OS seed (Makefile default)
make fuzz

# Long run with explicit seed
cargo run --release -p revive-fuzz -- --iterations 1000 --seed 42 --threads 8
```

Flags:

| Flag | Default | Effect |
|---|---|---|
| `-n, --iterations N` | `100` | Cases to run. `0` means "loop forever". |
| `-s, --seed N` | random | Master PRNG seed. Per-iteration seed = `seed.wrapping_add(iter)` so a finding at parallelism N reproduces at parallelism 1. |
| `-j, --threads N` | logical CPU count | Worker threads. |
| `--stop-on-divergence` | off | Exit 2 on the first divergence instead of accumulating. |
| `--verbose` | off | Print both observations on every iteration, not just divergences. |
| `--input-size N` | `4096` | Bytes of `arbitrary` tape per case. |
| `--output-dir DIR` | off | Emit-mode: write each generated `.sol` to `<DIR>/iter_<N>_<contract>.sol` without compiling. Useful for building a corpus for downstream tools. |
| `--direct-solc-evm` | off | Use direct `solc → EVM` instead of the default revive-yul roundtrip path. See [§EVM-side path](#evm-side-path). |

### Pure-random Yul (`revive-yul-fuzz`)

```bash
cargo run --release -p revive-fuzz --bin revive-yul-fuzz -- --iterations 1000 --seed 42
```

Same flag set as `revive-fuzz` (without `--direct-solc-evm` and
`--output-dir`). Generates Yul objects directly, bypassing solc's
Solidity → Yul step. Stresses resolc's Yul-input pipeline rather than
the Solidity frontend.

### Coverage-guided libFuzzer

```bash
# 4 forked workers, runs until interrupted
make fuzz-libfuzzer

# Tune fork count
make fuzz-libfuzzer JOBS=8

# Equivalent direct invocation (gives access to every libFuzzer flag)
cd fuzz
cargo +nightly fuzz run solidity_differential -- -fork=8
```

Useful libFuzzer flags (everything after the bare `--` is passed
through to the libFuzzer runtime):

| Flag | Effect |
|---|---|
| `-fork=N` | N parallel forked workers, sharing the corpus dir. |
| `-max_total_time=S` | Wall-clock budget in seconds. |
| `-runs=N` | Iteration budget instead of wall-clock. |
| `-max_len=N` | Cap input length (default 4096). |
| `-rss_limit_mb=N` | OOM threshold per worker (default 2048). |
| `-ignore_crashes=1` | Keep running after a crash instead of stopping. |
| `-print_final_stats=1` | Print coverage / corpus stats at exit. |

> [!NOTE]
>
> libFuzzer needs a nightly Rust toolchain. The `rust-toolchain.toml`
> inside `fuzz/` scopes nightly to that directory only — the rest of
> the workspace stays on stable.

### Reproducing a crash

libFuzzer writes crash inputs to
`fuzz/artifacts/solidity_differential/crash-<sha256>`. The bytes are
deterministic — re-feeding them produces the same `SolidityCase`:

```bash
cd fuzz
cargo +nightly fuzz run solidity_differential \
  artifacts/solidity_differential/crash-<sha256>
```

The panic message embeds the rendered contract source plus the action
sequence in hex — enough to file an issue without keeping the input
bytes around.

For the pure-random fuzzers: the divergence report prints the seed,
iteration index, and rendered source. `revive-fuzz -n 1 -s <seed>`
regenerates iter 0 with that seed; raise `-n` to reach a later
iteration.

## What the fuzzers do

```text
                random bytes (libFuzzer mutator, or ChaCha20 tape)
                            │
                            ▼
                Unstructured ──► TemplateKind::arbitrary
                            │
                            ▼  pick template + per-template op selectors
            SolidityCase { source, constructor_args, actions }
                            │
                ┌───────────┴────────────┐
                ▼                        ▼
         resolc::compile_blob       solc → EVM bytecode
         → PVM blob                 (direct or via revive-yul roundtrip)
                │                        │
                ▼                        ▼
      revive_runner::Specs.run     geth `evm` subprocess
      (pallet-revive sim)          (constructor + per-action calls)
                │                        │
                └───────────┬────────────┘
                            ▼
                       compare()
                  ├─ deploy_reverted flag
                  ├─ per-action revert flag
                  └─ per-action return-data bytes
                            │
                  any mismatch → Divergence
```

Both observers replay the same calldata sequence (`constructor_args`
concatenated as constructor input; `fn_0(uint256)` selector + 32-byte
arg per subsequent action). State carries across actions on both
backends. The harness compares revert flags and return-data bytes
only — gas-cost differences between geth `evm` and pallet-revive sim
are by design not part of the comparison.

## Coverage signal

cargo-fuzz compiles every Rust crate in the dep graph with
`-Cinstrument-coverage` and SanitizerCoverage, so the libFuzzer
mutation engine sees edges in:

* `revive-yul` parser/printer
* `resolc` standard-json pipeline
* `revive-llvm-context` codegen (every lowering pattern)
* `revive-runner` / pallet-revive simulation
* `arbitrary` and the generator itself

Opaque (not instrumented by default):

* `solc` subprocess
* geth `evm` subprocess
* LLVM C++ libraries inside resolc

> [!NOTE]
>
> `make install-llvm-coverage` rebuilds LLVM with source-based
> coverage (`LLVM_BUILD_INSTRUMENTED_COVERAGE=On`) for `llvm-cov`
> reports. It does **not** feed libFuzzer: that would require
> `LLVM_USE_SANITIZE_COVERAGE=On`, which triggers LLVM CMake to
> build `clang-fuzzer` and `clang-format-fuzzer` — standalone fuzz
> binaries that need `libclang_rt.fuzzer.a` to link. revive's
> compiler-rt runs **after** the LLVM tool build, so the runtime
> archive does not yet exist and the link fails. Wiring this up
> would mean either shipping a host-clang libFuzzer archive and
> pointing `LLVM_LIB_FUZZING_ENGINE` at it, or reordering the
> build so compiler-rt produces the runtime before the
> SanCov-instrumented LLVM tools link. Left as future work.

This is the right shape: the engine explores **resolc's** Rust-side
input space (and optionally LLVM internals), not solc's or geth's. A
5-minute run on 4 forks against a non-coverage LLVM loads ~1.17M
instrumented edges, of which the templated generator reaches ~28.5K.

## Generator

`crates/fuzz/src/templates.rs` defines eight contract templates. Each
exposes the same wire shape:

```solidity
constructor(uint256 seed) { ... }
function fn_0(uint256 arg) external returns (T) { ... }
```

so the observer doesn't need to know which template it's running.

| Kind | What it exercises |
|---|---|
| `Srem` | `int256` storage slot + `slot0 % arg` — the original [paritytech/revive#527](https://github.com/paritytech/revive/pull/527) probe |
| `ArithChain` | Two storage slots, three signed-arithmetic ops chained |
| `UncheckedArith` | `unchecked { … }` wrapping arithmetic on `uint256` |
| `Mapping` | `mapping(uint256 => uint256)` increment — exercises keccak-derived storage slots |
| `DynArray` | Dynamic `uint256[]` push + indexed read — exercises array layout + length update |
| `RequireGuard` | `require(predicate, "guard")` with eight predicate shapes |
| `LoopAccum` | Bounded `for` accumulator (`bound = arg & 0x1F`, ≤ 31 iterations) |
| `Bitwise` | Pure-bitwise composition (`& \| ^` + `<<` / `>>`) |

Op selectors inside each template are themselves `arbitrary`-driven,
so one template covers many distinct opcode lowerings. The
`#[ignore]`d `every_template_compiles` test pipes each template
through `solc --standard-json` and asserts no fatal errors:

```bash
cargo test -p revive-fuzz --lib -- --ignored every_template_compiles
```

A 256-bit boundary-value pool (`0`, `1`, `-1`, `INT_MIN`, `INT_MAX`,
`2^128`, `2^64`, alternating-bit patterns, …) is mixed into operands
with 1-in-5 probability so corner-case pairs surface within a minute
under pure-random. libFuzzer's mutator preserves the biasing because
it operates on the same byte tape `Unstructured` consumes.

## EVM-side path

The Solidity differential has two compile paths for the EVM side:

**1. revive-yul roundtrip** (`run_case`, default for the CLI driver).
solc emits Yul, `revive_yul` parses and reprints it, the reprinted
source is fed back to `solc --strict-assembly` for EVM bytecode.

Diagnoses: revive-yul printer bugs **or** resolc backend bugs.

**2. Direct solc** (`run_case_solc_evm`, used by the libFuzzer target
and by the CLI when `--direct-solc-evm` is passed). solc compiles
Solidity → EVM directly.

Diagnoses: only resolc backend bugs.

**Why libFuzzer uses the direct path:** the roundtrip path has a known
high noise floor — a single revive-yul printer bug repeats on every
input that exercises the relevant shape (e.g. dynamic-array `push`
clobbers the length update). Under libFuzzer's fork-restart loop that
manifests as a crash storm drowning out backend findings. A future
second fuzz target can specifically exercise revive-yul by comparing
`run_case` vs `run_case_solc_evm` on the same input; that isolates
revive-yul as the lone variable.

## Divergence taxonomy

`Divergence` (in `crates/fuzz/src/differential.rs`) categorises every
outcome:

| Variant | Meaning | libFuzzer treatment |
|---|---|---|
| `YulRoundtripCompile(msg)` | revive-yul rejected solc's Yul, **or** `solc --strict-assembly` rejected the reprinted source. Roundtrip path only. | n/a — the libFuzzer target uses the direct path. |
| `EvmCompile(msg)` | `solc → EVM` panicked. Direct path only. Almost always a **generator bug** (template emitted Solidity solc rejects). | **Silent skip** in the libFuzzer panic helper — doesn't burn a corpus slot. |
| `PvmCompile(msg)` | `resolc → PVM` panicked. solc accepted but resolc choked. | **Crash** — exactly the kind of resolc ICE the fuzzer is meant to find. |
| `DeployRevert { … }` | Constructor reverted on one backend but not the other. | Crash. |
| `ActionCount { … }` | Action result vectors of unequal length. Defensive; should not happen. | Crash. |
| `ActionRevert { … }` | One backend reverted on a call the other completed. | Crash. |
| `ActionReturnData { … }` | Both completed; return-data bytes differ. | Crash. |

Compile failures used to panic the whole process via
`.expect("source should compile")` inside resolc's `test_utils`. The
harness wraps both calls in `std::panic::catch_unwind` and routes the
payload into a dedicated variant, so a generator bug doesn't poison
the whole libFuzzer run.

## Performance

Templated Solidity, direct-solc path, on an M-class laptop:

| Step | Cost |
|---|---|
| `arbitrary → SolidityCase` | <1 ms |
| `solc → EVM` (subprocess) | ~80–100 ms |
| `resolc → PVM` (in-process) | ~150 ms |
| geth `evm` per action | ~10 ms × 2–4 actions |
| `revive_runner::Specs.run()` per action | ~5 ms |

≈ 250 ms / iter end-to-end. With `-fork=12`: ~30–40 iter/sec total.

Five-minute runs from an empty corpus, four forks:

| Generator | Iters | `cov` (edges) | `ft` (features) | Corpus |
|---|---|---|---|---|
| SREM-only | 14,077 | 26,669 | 32,392 | 101 |
| Templated, roundtrip EVM | aborted (279 crashes) | 29,022 | 47,578 | 148 |
| Templated, direct-solc EVM | 6,087 | **28,566** | **46,530** | **313** |

The templated generator opens ~2K more edges and 14K more features
than the SREM-only baseline, and keeps a 3× larger corpus.

> [!WARNING]
>
> libFuzzer is single-threaded per process. Use `-fork=N` for
> parallelism — not Rust threads, not rayon. Rayon inside a fuzz
> target would interleave coverage counters and produce useless data.

## Known limitations

* **Subprocess overhead dominates.** `solc` + `evm` subprocess costs
  cap throughput at ~30 iter/sec on 12 cores. A native-Rust EVM on the
  EVM side would be ~10× faster but is out of scope.
* **One external function shape.** The harness hardcodes
  `fn_0(uint256)` so the observer doesn't have to vary calldata
  encoding. Removing that assumption requires generalising
  `observe::action_calldata`.
* **Solc internals are opaque.** libFuzzer can't see solc's Yul
  optimiser. Fine for resolc-side bug finding; not useful for solc
  bug finding.
* **Stack traces aren't captured** in `catch_unwind` payloads. Easy
  follow-up to wire `std::backtrace::Backtrace::capture()` through.
* **revive-yul printer bugs are out of scope** for the libFuzzer
  target by design — they're filed against `run_case` (the CLI
  roundtrip path) instead.

## Code map

```text
crates/fuzz/
├── Cargo.toml                       # `panic-on-divergence` feature
├── src/
│   ├── lib.rs                       # re-exports + `panic_on_divergence` helpers
│   ├── generator.rs                 # SolidityCase + Arbitrary impl
│   ├── templates.rs                 # 8 template renderers + solc self-test
│   ├── pipeline.rs                  # solc / resolc invocation helpers
│   ├── observe.rs                   # observe_evm / observe_pvm
│   ├── differential.rs              # Divergence + run_case + run_case_solc_evm
│   ├── yul/                         # Yul-input variant
│   └── bin/
│       ├── revive_fuzz.rs           # pure-random Solidity CLI
│       └── revive_yul_fuzz.rs       # pure-random Yul CLI

fuzz/                                # cargo-fuzz package (separate workspace)
├── Cargo.toml                       # libfuzzer-sys + path-dep on revive-fuzz
├── rust-toolchain.toml              # nightly, scoped here only
└── fuzz_targets/
    └── solidity_differential.rs     # libFuzzer entry
```
