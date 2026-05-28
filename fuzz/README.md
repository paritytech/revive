# revive-fuzz-libfuzzer

Coverage-guided fuzzer for the Solidity differential, driven by
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) / libFuzzer.
Each iteration generates a Solidity contract, compiles it with both
`solc` and `resolc`, executes both, and panics on any divergence in
revert flags or return data.

The corpus is shared between runs, so coverage accumulates over time.

## Prerequisites

* Nightly Rust — pinned by `rust-toolchain.toml` in this directory only.
  The workspace stays on stable.
* `LLVM_SYS_221_PREFIX` — same compatible LLVM 18.1.8 build as
  `make test-workspace`.
* `solc` and geth's `evm` on `$PATH` — the harness shells out for the
  EVM-side compile and execution.
* `resolc` installed to `~/.cargo/bin/` via `make install-bin` — the
  harness resolves it via `which::which`.

## Running

The fastest way:

```bash
make fuzz-libfuzzer
make fuzz-libfuzzer JOBS=8           # shard across 8 forked workers
```

Equivalent direct invocation:

```bash
cd fuzz
cargo +nightly fuzz run solidity_differential -- -fork=8
```

Useful libFuzzer flags (everything after the bare `--` is passed
through to libFuzzer):

```bash
-fork=N             # N parallel forked workers, sharing the corpus
-max_total_time=S   # wall-clock budget in seconds
-runs=N             # iteration budget instead of wall-clock
-max_len=N          # cap input length (default 4096)
-rss_limit_mb=N     # OOM threshold per worker (default 2048)
```

## What's instrumented

Every Rust crate in the fuzz target's dep graph builds with
SanitizerCoverage, so libFuzzer sees edges in:

* `revive-yul` parser/printer
* `resolc` standard-json pipeline
* `revive-llvm-context` codegen
* `revive-runner` / pallet-revive simulation

`solc` and `evm` are subprocesses and therefore opaque to libFuzzer —
which is fine. The mutation engine is exploring **resolc's** input
space, not solc's.

## EVM-side path

The libFuzzer target uses [`run_case_solc_evm`](../crates/fuzz/src/differential.rs)
— direct `solc → EVM`. Pure backend-vs-backend; findings point at
`resolc → PVM`.

## Generator

The Solidity generator in
[`crates/fuzz/src/templates.rs`](../crates/fuzz/src/templates.rs)
picks uniformly from eight contract templates:

* `Srem` — `int256` storage slot, `slot0 % arg` (the original
  paritytech/revive#527 probe);
* `ArithChain` — two storage slots, three signed-arithmetic ops
  chained;
* `UncheckedArith` — `uint256` with `unchecked { … }` wrapping
  arithmetic;
* `Mapping` — `mapping(uint256 => uint256)` increment;
* `DynArray` — dynamic `uint256[]` push + indexed read;
* `RequireGuard` — `require(predicate, "guard")` with one of eight
  boolean predicate shapes;
* `LoopAccum` — bounded `for` accumulator (`bound = arg & 0x1F`,
  so ≤ 31 iterations);
* `Bitwise` — pure bitwise composition (`& | ^` + `<<` / `>>`).

Op selectors inside each template are also `arbitrary`-driven, so
the same template covers many distinct opcode lowerings. Adding a
ninth template that exercises a new opcode immediately becomes
coverage-rewarded — no other plumbing is needed.

## Corpus, crashes, repro

```
fuzz/
├── corpus/solidity_differential/      # inputs that grew coverage
└── artifacts/solidity_differential/   # inputs that triggered a panic
    └── crash-<sha256>
```

To re-run a single crash file (deterministic — same bytes always
yield the same `SolidityCase`):

```bash
cd fuzz
cargo +nightly fuzz run solidity_differential \
  artifacts/solidity_differential/crash-<sha256>
```

The panic message includes the rendered Solidity source, constructor
args, and action sequence — enough to file an issue without keeping
the byte-level input around.

## Cleaning up

```bash
cd fuzz
cargo fuzz cmin solidity_differential   # shrink the corpus
rm -rf target/                          # nuke build artifacts
rm -rf corpus/ artifacts/               # nuke discovered inputs (rarely wanted)
```
