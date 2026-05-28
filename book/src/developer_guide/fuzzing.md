# Differential fuzzing

`revive` ships a coverage-guided differential fuzzer that compares the
same logical contract execution between resolc's PVM lowering and
solc's EVM lowering. Any byte-level mismatch on revert flags or
return data is treated as a backend bug.

The harness uses libFuzzer with SanitizerCoverage feedback over every
Rust crate in resolc's dep graph, so the mutation engine learns from
edge coverage in the parser, IR lowering, and pallet-revive
simulation.

> [!TIP]
>
> The fuzzer shells out to `solc` and geth's `evm`. Both must be on
> `$PATH`. See the [Testing strategy](./testing.md) chapter for the
> geth EVM-tool installation snippet.

## Running the fuzzer

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
> libFuzzer needs a nightly Rust toolchain because the SanitizerCoverage
> flags it relies on (`-Zsanitizer-coverage-*`, `-Cpasses=sancov-module`,
> etc.) are nightly-only. The `rust-toolchain.toml` inside `fuzz/`
> scopes nightly to that directory — the rest of the workspace stays
> on stable.

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

## What the fuzzer does

```text
                  libFuzzer mutator (random bytes)
                            │
                            ▼
                Unstructured ──► TemplateKind::arbitrary
                            │
                            ▼  pick template + per-template op selectors
            SolidityCase { source, constructor_args, actions }
                            │
                ┌───────────┴────────────┐
                ▼                        ▼
         resolc → PVM blob           solc → EVM bytecode
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
                  any mismatch → Divergence → panic
```

Both observers replay the same calldata sequence (`constructor_args`
concatenated as constructor input; `fn_0(uint256)` selector + 32-byte
arg per subsequent action). State carries across actions on both
backends. The harness compares revert flags and return-data bytes
only — gas-cost differences between geth `evm` and pallet-revive sim
are by design not part of the comparison.

## Coverage signal

cargo-fuzz compiles every Rust crate in the dep graph with
SanitizerCoverage, so the libFuzzer mutation engine sees edges in:

* `revive-yul` parser
* `resolc` standard-json pipeline
* `revive-llvm-context` codegen (every lowering pattern)
* `revive-runner` / pallet-revive simulation
* `arbitrary` and the generator itself

Opaque (not instrumented by default):

* `solc` subprocess
* geth `evm` subprocess
* LLVM C++ libraries inside resolc

To extend SanCov instrumentation into LLVM's C++ codebase, rebuild
LLVM via `make install-llvm-sancov`. That target adds
`-fsanitize=fuzzer-no-link` to LLVM's C/CXX flags, so every basic
block in the resulting static archives carries libFuzzer edge
counters. The `-no-link` suffix avoids requiring `libclang_rt.fuzzer.a`
at LLVM-build time — the libFuzzer runtime comes from the fuzz-target
binary.

> [!WARNING]
>
> A SanCov-instrumented LLVM at `$LLVM_SYS_221_PREFIX` will break
> non-fuzz `cargo build` invocations: the linker needs
> `__sanitizer_cov_*` symbols that only the libFuzzer runtime
> supplies. Keep two LLVM trees if you need both: switch via
> `LLVM_SYS_221_PREFIX`.

This is the right shape: the engine explores **resolc's** Rust-side
input space (and optionally LLVM internals), not solc's or geth's. A
5-minute run on 4 forks against a non-instrumented LLVM loads ~1.17M
edges, of which the templated generator reaches ~28.5K.

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

## Divergence taxonomy

`Divergence` (in `crates/fuzz/src/differential.rs`) categorises every
outcome:

| Variant | Meaning | libFuzzer treatment |
|---|---|---|
| `EvmCompile(msg)` | `solc → EVM` panicked. Almost always a **generator bug** (template emitted Solidity solc rejects). | **Silent skip** in the libFuzzer panic helper — doesn't burn a corpus slot. |
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

Templated Solidity on an M-class laptop:

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
| Templated | 6,087 | **28,566** | **46,530** | **313** |

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
* **Recursive resolc isn't instrumented.** `resolc::test_utils` spawns
  the installed `~/.cargo/bin/resolc` as a subprocess via
  `--recursive-process` for per-contract lowering. Only the
  in-process call sites carry SanCov instrumentation; the subprocess
  is opaque to libFuzzer. `revive_fuzz::warn_if_resolc_stale` logs a
  warning when the installed binary is older than workspace source,
  to flag the case where a local fix isn't visible to the fuzzer.
* **One external function shape.** The harness hardcodes
  `fn_0(uint256)` so the observer doesn't have to vary calldata
  encoding. Removing that assumption requires generalising
  `observe::action_calldata`.
* **Solc internals are opaque.** libFuzzer can't see solc's Yul
  optimiser. Fine for resolc-side bug finding; not useful for solc
  bug finding.
* **Stack traces aren't captured** in `catch_unwind` payloads. Easy
  follow-up to wire `std::backtrace::Backtrace::capture()` through.

## Code map

```text
crates/fuzz/                         # revive-fuzz harness library (stable Rust, main workspace)
├── Cargo.toml                       # `panic-on-divergence` feature
├── src/
│   ├── lib.rs                       # re-exports + `panic_on_divergence` helper
│   ├── generator.rs                 # SolidityCase + Arbitrary impl
│   ├── templates.rs                 # 8 template renderers + solc self-test
│   ├── pipeline.rs                  # solc / resolc invocation helpers
│   ├── observe.rs                   # observe_evm / observe_pvm
│   ├── differential.rs              # Divergence + run_case_solc_evm
│   └── stale.rs                     # `warn_if_resolc_stale`

fuzz/                                # cargo-fuzz package (separate workspace, nightly)
├── Cargo.toml                       # libfuzzer-sys + path-dep on revive-fuzz
├── rust-toolchain.toml              # nightly, scoped here only
└── fuzz_targets/
    └── solidity_differential.rs     # libFuzzer entry
```

The split exists because cargo-fuzz needs a nightly toolchain and
pulls in `libfuzzer-sys` — keeping that in a separate workspace
prevents either from leaking into the main `cargo build`.
