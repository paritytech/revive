# llm-corpus-gen

LLM-driven Solidity seed-corpus generator for the revive differential fuzzer's
**source-text** target (`solidity_source_differential`).

It prompts an LLM for self-contained Solidity contracts that match the fuzzer's
wire shape, validates each one, and writes accepted contracts as
content-addressed `.sol` files. The fast byte-level fuzzing engines then mutate
those seeds — the LLM stays *off the hot path*, enriching the corpus rather than
generating every input.

Standalone crate (own `[workspace]`), deliberately outside the revive workspace
so its HTTP/TLS deps never enter the instrumented fuzzer build.

## Wire shape

Every generated contract must expose what the differential oracle drives:

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8;
contract Fuzz {
    constructor(uint256 seed) { /* ... */ }
    function fn_0(uint256 arg) external returns (uint256) { /* ... */ }
}
```

The system prompt also **forbids environment-dependent behaviour**
(`block.*`, `msg.*`, external calls, timestamps, randomness, …) because those
would produce false EVM/PVM divergences.

## Usage

```bash
# Claude Code (default). NO API key — reuses the `claude` CLI's existing auth.
# Just needs Claude Code installed on PATH; solc on $PATH validates output.
cargo run --release -- --out ../../llm-corpus --count 5

# Steer generation at a specific codepath (Component 4 feedback loop).
cargo run --release -- --focus "the sdiv lowering path in revive-llvm-context"

# Anthropic API instead of the CLI (requires a key).
export ANTHROPIC_API_KEY=sk-ant-...
cargo run --release -- --provider claude

# Custom OpenAI-compatible endpoint.
export LLM_API_KEY=...
cargo run --release -- --provider custom \
    --endpoint https://my-host/v1/chat/completions --model my-model
```

Or via the repo Makefile (also copies seeds into the fuzzer corpus):

```bash
make llm-corpus COUNT=8 FOCUS="unchecked mul overflow"
```

### Flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--out` | `llm-corpus` | Output directory for `.sol` files |
| `--count` | `5` | Contracts requested per risk area |
| `--provider` | `claude-code` | `claude-code` (no key), `claude` (API key), or `custom` |
| `--model` | provider default | Model id (optional for `claude-code`) |
| `--endpoint` | `$LLM_ENDPOINT` | Chat-completions URL (custom) |
| `--max-tokens` | `2048` | Generation cap |
| `--focus` | – | Extra prompt guidance |
| `--no-solc` | off | Skip the real `solc` compile check |
| `--avoid-dir` | – | Sample existing contracts here; ask for structurally *different* ones (diversify mode) |
| `--avoid-count` | `4` | How many existing contracts to show as "avoid these" |
| `--avoid-maxlen` | `800` | Per-sample char cap for the avoid examples |

## Validation gate

1. **Structural** — must contain `contract `, `constructor(uint256`, and
   `fn_0(uint256`.
2. **`solc` compile** — only contracts solc accepts are written. A contract solc
   accepts but `resolc` later rejects is a *real find*, so it is intentionally
   **not** filtered here.

Duplicates are dropped by content hash (filenames are a truncated SHA-256).

## The feedback loop

`loop.sh` fuzzes, and **only regenerates when a round finds nothing**:

1. Fuzz the source-text target for `DURATION` seconds.
2. If a finding was recorded (the harness writes reproducers to `fuzz/findings/`),
   **stop** — the reproducer `.sol`/`.txt` are there.
3. If not, ask the LLM for a new batch that is **structurally different** from the
   current corpus (via `--avoid-dir <corpus>`, which samples previous contracts
   into the prompt), add them to the corpus, and fuzz again.
4. Repeat until a finding appears or `ROUNDS` is reached.

```bash
ROUNDS=10 DURATION=600 JOBS=8 FOCUS="srem INT_MIN" ./loop.sh
```

Knobs: `ROUNDS`, `DURATION`, `JOBS`, `COUNT`, `FOCUS`, and `TIMEOUT` (per-input
wall-clock cap, default 30s — bounds one pathological seed so it can't stall the
fork-mode corpus merge).

This closes the loop: the fuzzer mutates a valid corpus at full speed, and when a
window is exhausted without a bug, the LLM injects genuinely new contract shapes
rather than re-seeding the same ones.

> **Keep the corpus small.** This harness compiles on every execution, so fork
> mode's per-round merge over a large corpus is slow. If the corpus grows into
> the thousands, minimize it with `make fuzz-cmin` (coverage-preserving) so each
> round starts fuzzing quickly.

## Durable findings

Because libFuzzer's fork mode does not reliably persist crash artifacts, the
harness writes every divergence to `fuzz/findings/<hash>.sol` (a directly
reproducible contract) plus `<hash>.txt` (full divergence detail) *before*
aborting. Override the location with `REVIVE_FUZZ_FINDINGS_DIR` (the `make
fuzz-libfuzzer*` targets and `loop.sh` set it to an absolute `fuzz/findings`).
