#!/usr/bin/env bash
# LLM-in-the-loop fuzzing for the source-text differential target.
#
# Each round: fuzz for DURATION seconds. If a finding is recorded (the harness
# writes reproducers to fuzz/findings/), stop. If not, ask the LLM for a NEW
# batch of contracts that are STRUCTURALLY DIFFERENT from the current corpus —
# feeding it samples of the previous corpus via `--avoid-dir` — add them, and
# fuzz again. Repeats until a finding appears or ROUNDS is reached.
#
# Env knobs:
#   ROUNDS    max fuzz/regenerate cycles                (default 10)
#   DURATION  seconds of fuzzing per round              (default 600)
#   JOBS      forked libFuzzer workers                  (default 8)
#   COUNT     contracts per risk area per regeneration  (default 5)
#   FOCUS      free-text guidance appended to prompts   (default empty)
#   TIMEOUT    per-input wall-clock cap in seconds       (default 30)
#   CORPUS_MAX cap on working-corpus size before a round (default 300)
#
# CORPUS_MAX keeps rounds responsive: this harness compiles on every execution
# (~1 exec/s), so fork mode's per-round merge over a large corpus dominates the
# window. Excess (oldest) inputs are moved to fuzz/corpus-archive/ (reversible),
# not deleted — restore with `mv fuzz/corpus-archive/<target>/* <corpus>/`.
set -euo pipefail

ROUNDS="${ROUNDS:-10}"
DURATION="${DURATION:-600}"
JOBS="${JOBS:-8}"
COUNT="${COUNT:-5}"
FOCUS="${FOCUS:-}"
TIMEOUT="${TIMEOUT:-30}"
CORPUS_MAX="${CORPUS_MAX:-300}"

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

target="solidity_source_differential"
corpus="fuzz/corpus/$target"
findings="$repo_root/fuzz/findings"
archive="$repo_root/fuzz/corpus-archive/$target"
gen_manifest="tools/llm-corpus-gen/Cargo.toml"

mkdir -p "$corpus" "$findings"

focus_args=()
[ -n "$FOCUS" ] && focus_args=(--focus "$FOCUS")

count_findings() { find "$findings" -type f 2>/dev/null | wc -l | tr -d ' '; }

# Keep the newest CORPUS_MAX inputs; archive the rest so the merge stays fast.
trim_corpus() {
    local total
    total="$(ls -1 "$corpus" 2>/dev/null | wc -l | tr -d ' ')"
    [ "$total" -le "$CORPUS_MAX" ] && return 0
    echo "== trimming corpus ${total} -> ${CORPUS_MAX} (archiving oldest $((total - CORPUS_MAX))) =="
    mkdir -p "$archive"
    ls -t "$corpus" | tail -n +"$((CORPUS_MAX + 1))" | while IFS= read -r file; do
        mv "$corpus/$file" "$archive/" 2>/dev/null || true
    done
}

generate() {
    # $@ are extra generator args (e.g. --avoid-dir). Writes into the corpus.
    # `${arr[@]+"${arr[@]}"}` expands safely when empty under `set -u` (bash 3.2).
    cargo run --release --manifest-path "$gen_manifest" -- \
        --out "$corpus" --count "$COUNT" ${focus_args[@]+"${focus_args[@]}"} "$@"
}

# Seed once if the corpus is empty.
if [ -z "$(ls -A "$corpus" 2>/dev/null)" ]; then
    echo "== seeding empty corpus =="
    generate
fi

for round in $(seq 1 "$ROUNDS"); do
    trim_corpus
    before="$(count_findings)"
    echo "== round ${round}/${ROUNDS}: fuzzing ${DURATION}s (JOBS=${JOBS}); corpus=$(ls -1 "$corpus" | wc -l | tr -d ' '), findings so far: ${before} =="
    # `-timeout` bounds each unit so one pathological seed can't stall the
    # fork-mode corpus merge (libFuzzer's default is 1200s, which looks hung).
    ( cd fuzz && REVIVE_FUZZ_FINDINGS_DIR="$findings" \
        cargo +nightly fuzz run "$target" -- \
        -dict=solidity.dict -fork="$JOBS" -ignore_crashes=1 \
        -timeout="$TIMEOUT" -max_total_time="$DURATION" ) || true

    after="$(count_findings)"
    if [ "$after" -gt "$before" ]; then
        echo "== FOUND new finding(s); stopping. Reproducers in ${findings}/ =="
        ls -t "$findings" | head
        exit 0
    fi

    echo "== round ${round}: no findings in ${DURATION}s; regenerating corpus diversified against the previous one =="
    generate --avoid-dir "$corpus"
done

echo "== loop complete: ${ROUNDS} rounds, no findings =="
