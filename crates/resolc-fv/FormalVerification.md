# Formal verification of the newyork optimizer

Working notes for the formal-verification (FV) feature. This file is deliberately
separate from `book/` — the feature is not stable yet, and the user-facing
documentation gets written once it is.

## What the feature is for

The newyork optimizer rewrites the IR a pass at a time. Each rewrite is supposed
to preserve the program's meaning, and nothing checks that today beyond the
differential test suite, which exercises whole compilations rather than
individual passes.

The FV feature makes the optimizer's intermediate steps observable, so that each
pass can be checked on its own: `resolc` exports the IR after every pass, and
`resolc-fv` consumes that chain and discharges one equivalence obligation per
adjacent pair of stages. A failure then points at a single pass instead of at
"the compiler".

## The two halves

### Export: `resolc --emit-ir-stages`

Requires `--debug-output-dir`. Writes, into that directory:

- one artifact per optimization pass, holding the IR at that point plus the side
  tables codegen reads (heap analysis, inferred types, inlining decisions,
  memory counters) — the IR alone does not determine the generated code;
- a manifest indexing those artifacts in pipeline order.

The sink is in `crates/newyork/src/export.rs`; it performs no I/O itself, since
that crate sits below `revive-llvm-context` in the dependency order. The compiler
supplies the writer, in `crates/resolc/src/project/contract/ir/newyork.rs`.

Stage identifiers look like `007.fold_keccak` or `019.late0.simplify` — ordinal,
optional loop scope, pass name. They sort lexically into pipeline order. A
consumer should key on the `pass` field rather than the ordinal, which shifts
whenever the pipeline gains or loses a pass.

Each stage carries an `ir_hash` over the serialized IR alone, excluding the side
tables. Equal hashes across adjacent stages of one object mean the pass left the
IR untouched — though it may still have moved a side table. The compiler also
asserts that the last exported stage is the IR codegen actually received, so a
chain cannot describe a compilation that did not happen.

### Consumption: `resolc-fv`

`crates/resolc-fv`, installed with `make install-fv`. Built separately from the
compiler: it is a consumer of the artifacts, not part of the compilation
pipeline, and it is expected to pull in dependencies (solvers and the like) that
`resolc` has no business carrying.

It runs after `resolc`, against an existing output directory. The two never run
concurrently and `resolc-fv` never invokes the compiler.

**Status: scaffold.** It takes `--stages <dir>`, parses every `*.stages.json`
manifest in that directory along with the stage artifacts each one indexes,
prints the resulting chain, plans one obligation per step and runs them
`--jobs` at a time behind a live overview. The checkers themselves are stubs, so
every obligation comes back unproven.

### Module layout

```
main.rs              the CLI and the wiring
common.rs            the vocabulary: Chain, Stage, Obligation, Proof, Verdict, …
parse.rs             reading a compilation's artifacts back into a Chain
serialize.rs         writing out what a prover concluded
output.rs            the listing, the live overview, the report
verification/
  mod.rs             stating the obligations and running them
  yul_newyork.rs     the obligation the Yul translation carries
  newyork.rs         the obligation every optimization pass carries
  newyork_llvm.rs    the obligation codegen carries
  simulation.rs      the stand-in solver, which goes with the stubs
  smt.rs             talking to a real SMT solver
```

`common` depends on nothing in the crate; everything else depends on it. Each
checker is its own module so that a real implementation can grow there without
touching the dispatch, and each exposes the same `is_equivalent` name — the
module path is what distinguishes them.

### Running the proofs

Obligations are independent — each compares two artifacts already on disk — so
the only bound on concurrency is how many provers the machine should run.
`-j`/`--jobs` sets that, defaulting to the available parallelism. The provers
share nothing but a cursor into the work list; verdicts are keyed by obligation
index, which keeps the report in pipeline order however the provers interleave.

The overview (via `indicatif`) draws one line per contract, carrying one symbol
per obligation in pipeline order, so a symbol's position is the position of the
step it stands for:

| Symbol | Colour | State |
| --- | --- | --- |
| `·` | dim | pending |
| `▸` | cyan | running |
| `✓` | green | proven correct |
| `✗` | red | proven incorrect |
| `?` | yellow | unknown |

The summary uses the same three colours for its counts, and `FAILED` is red, so
a number and the symbols it counts always agree. A count of zero is dimmed
rather than coloured — a red `0 failed` reads as a problem at a glance when it
is the opposite of one.

Colour is applied per symbol rather than through the progress template, since a
line carries one symbol per obligation and each has its own state. Whether the
escapes are emitted at all is left to `console` — the same crate, and so the
same check, `indicatif` uses for its own drawing, so the two cannot disagree
about what the terminal takes. Piped output, `NO_COLOR` and `TERM=dumb` all come
out plain. The overview draws to stderr and the legend to stdout, so the two are
decided against their own streams.

An obligation whose other side is missing still appears, carrying the reason it
could not be stated. A step silently dropped from the overview would leave the
run looking more complete than it is.

Each stage is modelled as a transformation — `Stage { index, from, to, content }`,
where `from` and `to` are `Format` values and `content` is the parsed
`StageRecord`. `index` is the manifest's order, which is the order the passes ran
in. Only the first stage reads anything other than newyork IR: `from_yul`
consumes the Yul AST, while every optimization pass rewrites the IR in place. The
pass name that marks it is `revive_newyork::PASS_FROM_YUL`, shared with the
compiler that writes it rather than spelled out again on this side.

Two consistency checks run while parsing. A manifest whose `schema` is not the
version this build knows is rejected rather than parsed, so a compiler that moved
a field reports a version mismatch instead of a confusing parse failure. And a
stage artifact whose `stage_id` disagrees with the manifest entry pointing at it
is an error — that is what a directory holding the output of two different
compilations looks like.

### The obligations

`verify_chain` states one obligation per stage, plus one for codegen, and hands
each to `is_equivalent`, which dispatches on the pair of formats — a Yul AST
against newyork IR is a different statement from newyork IR against newyork IR,
so each pair has its own checker. All three are stubs:

| From | To | Checker |
| --- | --- | --- |
| `yul` | `newyork` | `is_equivalent_yul_newyork` — the `from_yul` translation |
| `newyork` | `newyork` | `is_equivalent_newyork` — every optimization pass |
| `newyork` | `llvm` | `is_equivalent_newyork_llvm` — codegen |

The codegen obligation closes the chain. Its newyork side is the manifest's last
stage, which the compiler guarantees is the IR codegen received, and its LLVM
side is the `<stem>.unoptimized.ll` dump beside the manifest. It is listed as a
final row after the stages, indexed one past the last of them, so the listing
has one row per obligation and the overview's symbols line up against those rows
position for position. The row reads `(nothing)` when no dump was written. The unoptimized
dump is codegen's direct output; `<stem>.optimized.ll` has already been through
LLVM's own passes, which is a separate obligation this tool does not state. The
codegen dumps are not listed in the manifest — they share its file-name stem,
which is what locates them.

A pass is checked against **the previous stage of the same object**. A contract's
deploy and runtime halves run the same pass sequence, and pairing across that
boundary would compare unrelated code. The Yul translation is checked against the
Yul AST, the input it actually consumed, which the manifest's `yul_ast` names.
A stage that opens an object's chain with no predecessor — a subobject's first
pass, whose input sits inside the parent object's tree — is counted unproven
rather than skipped.

A verdict is `Equivalent`, `NotEquivalent` with a counterexample, or `Unproven`
with a reason. The third is deliberately not folded into the first: a verifier
that reported an obligation it never examined as holding would be worse than one
that reported nothing, because the output would read as a proof. The run prints
a `proved / failed / unproven` summary, says so plainly while the checks are
stubs, and exits non-zero only when an obligation actually failed.

### Proof logs

A checker returns a `Proof`: its verdict plus the transcript it recorded getting
there. Given `--log-dir`, every obligation's proof is written to its own file,
`<stem>.<step>.fv.log`, under that directory. It is created before any proving
starts, so a bad path fails immediately rather than after a long run.

There is no default. Writing logs means writing into a directory the caller did
not nominate, and the artifact directory belongs to the compiler — so without
`--log-dir` the proofs still run and their logs are discarded. A run says so
twice, before and after, since a long run should not finish only to reveal that
its evidence was never kept.

```
step: 007.fold_keccak -> 008.simplify
transform: newyork -> newyork
verdict: refuted
counterexample: ...
---
check: the newyork pass equivalence check
solver: simulated
elapsed: 2373 ms
result: refuted
```

`step` names both ends of the transition, not just the stage that came out, so a
log says what was compared against what. Each end is named by something the
reader can go and open — a stage identifier, the dumped Yul AST at the start of
a chain, the codegen dump at the end — or `(nothing)` where that side does not
exist:

```
step: Token.yul.json -> 000.from_yul            the Yul translation
step: 007.fold_keccak -> 008.simplify           an optimization pass
step: 052.final -> Token.unoptimized.ll         codegen
step: (nothing) -> 016.inline                   a subobject's first pass
```

The header carries the verdict in its own right, along with the counterexample
that refuted the step or the reason nothing was shown, rather than relying on
the transcript to mention them — so a log stays readable whatever a future
checker chooses to record. Obligations that were never attempted get a
log too, saying why — the same reasoning that keeps them in the overview.

Each prover writes its own file, so there is nothing to serialize between
threads. A write that fails is collected and reported after the run rather than
printed where it happens, which would tear the live overview.

### The SMT backend

`--smt-backend` picks the solver the checks talk to: `z3` (the default) or
`cvc5`. The binding is [`smtlib`](https://github.com/oeb25/smtlib-rs).

The solver is a binary, installed separately, the way `solc` is. It follows the
pattern `solc` already uses here: a default name per backend, a `--smt-solver
<path>` override, and a `which` lookup at startup that fails with the same
wording as `SolcCompiler::new` —

```
The `z3` executable not found in ${PATH}: cannot find binary path
```

— resolved once before any obligation runs, rather than once per obligation
afterwards. A solver is mandatory: the run stops there.

Linking a solver in was tried and dropped. `smtlib`'s `z3-static` feature would
have removed the prerequisite entirely, but the backend it enables leaks its Z3
context — see [Upstream bugs](#upstream-bugs) below.

Nothing needs a solver yet, so what the backend currently gets is a probe: a
fixed query carrying no contract data, run once per obligation that reaches a
checker, whose answer is recorded at the head of the proof's log.

```
smt: z3 answered sat (x = 204, y = 179)
```

The query has exactly one solution, so a wrong answer is as visible as no answer
at all. A solver that is not installed is reported rather than raised —

```
smt: z3 did not answer: No such file or directory (os error 2)
```

— since it costs the run nothing but the probe, and a missing solver is a thing
to read in the log beside the obligation it would have discharged. Obligations
that never reach a checker, because one side of the step is missing, invoke no
solver at all: there is nothing to ask about.

### Simulated solvers

The three checkers are stubs: each sleeps for a random time drawn from
`SIMULATED_SOLVER_MILLIS` and returns a random verdict — roughly 70% proved, 10%
refuted, the rest unknown, which is what a solver that times out looks like.
That is enough for the rest of the tool to be exercised against something that
behaves like a solver: obligations finishing out of order, a running frontier
the width of `--jobs`, counts climbing. No run means anything yet.

The randomness is a counter through the splitmix64 finalizer, seeded from the
standard library's randomly keyed hasher. It is deliberately not a dependency:
nothing here needs statistical quality, only variety across runs and threads.
It goes when the real checks arrive.

## Visualizing a chain

`cfg-viz/`, at the repository root, renders the control flow of a stage
artifact. It answers a different question from the proofs — not "did this pass
preserve meaning" but "what does the IR at this point actually look like" —
against the same files, so a chain exported for `resolc-fv` needs nothing extra
to be viewed.

The newyork IR has no control flow graph in it. It stores *structured* regions:
`If` holds a then-region and an else-region, `Switch` holds one per case, `For`
holds a condition, a body and a post, and values cross a region boundary through
`inputs` and the region's `yields`. The viewer therefore derives a graph rather
than reading one, lowering regions to basic blocks the way
`crates/newyork/src/to_llvm.rs` does, so the picture matches what codegen builds
rather than merely resembling it.

### Running it

The viewer is a static site. It is not part of the Rust workspace, has no
dependencies beyond Python 3 and a browser, and needs no LLVM build — it reads
artifacts, never the compiler.

```bash
cd cfg-viz
python3 -m http.server 8000
```

Then open `http://localhost:8000`. Opening `index.html` from disk does not work:
the page fetches its data, and `file://` origins are not allowed to. Everything
else is served locally, including the layout engine in `vendor/`, so the viewer
works offline — only the webfont link needs the network, and the page falls back
to system faces without it.

### Pointing it at a contract

```bash
resolc MyContract.sol --newyork --overwrite --bin \
  --emit-ir-stages --debug-output-dir stages-mine/

cd cfg-viz
python3 bundle.py ../stages-mine ../stages-other
```

`bundle.py` takes any number of stage directories, lowers every stage each one
indexes, and writes `data/`. It merges into the index rather than replacing it,
so rebuilding one contract leaves the other chains in the picker alone — it
prints a line per chain saying which were rebuilt and which were kept. A chain
leaves the picker by deleting its file from `data/`, which the next run reports.

The two scripts underneath it are useful on their own:

| Command | What it does |
| --- | --- |
| `python3 lower.py <stage>.newyork.json` | Lowers one stage, writes `cfg.json` to stdout |
| `python3 lower.py <stage>.newyork.json out.json` | The same, to a file, with per-graph block and edge counts |
| `python3 validate.py out.json` | Checks the lowering's invariants; exits non-zero on a violation |
| `node smoke.js` | Renders every stage of every bundled chain against a stub DOM |

`validate.py` is the one to reach for when a new contract produces something
unexpected. It checks that every edge lands on a block that exists, that every
block is reachable from the entry, that every phi source is a real predecessor
carrying a resolved value, and that the back edges and the loop headers agree in
number. A violation is a lowering bug, not a quirk of the contract.

### Reading the graph

Each graph is one object's code or one function body; the rail lists them, and
`inline` removing a function is visible as that function leaving the list.

Edge kind carries the meaning, so it owns the colour: taken and not-taken
branches, the back edge of a loop, and the non-local jumps `Break`, `Continue`
and `Leave`, which are the edges that make the structured IR a graph rather than
a tree. Terminators — `Return`, `Revert`, `PanicRevert` and the rest — merge
into one sink per kind, because a contract of any size otherwise renders as a
bush of revert leaves.

A **dashed outline** marks a block the lowering invented: join blocks, loop
headers, latches and exits, and the implicit arms of an `If` with no
`else_region` or a `Switch` with no `default`. Those two are the common case,
not an edge case — in the `LoopSwitch` fixture 13 of 14 `If`s carry no else
region at `from_yul` and 14 of 15 at `final`, and its selector `Switch` has no
default at any stage. Skipping them loses an incoming edge at the join, which is
where the phis come from.

The pass rail marks the stages that actually rewrote the IR, which the manifest's
`ir_hash` decides: equal hashes across adjacent stages of one object mean the
pass left the IR untouched, and most passes do — 6 of 43 adjacent pairs changed
anything in the `LoopSwitch` fixture, 4 of 43 in `Add3`. The rail also breaks
the chain by object, since
a contract's deploy and runtime objects run the same pass sequence and the
ordinals interleave them.

### Status and the port

**Status: prototype.** The lowering is Python, under `cfg-viz/lower.py`. It
belongs in Rust behind a `resolc-fv graph` subcommand, where it can deserialize
`revive_newyork::ir` directly instead of walking untyped JSON, and where `parse.rs`
already does the chain reading it currently repeats. The `cfg.json` files it
writes are the snapshots that port has to reproduce.

What it is checked against today:

- Every stage of both fixtures lowers with no invariant violation, and
  `node smoke.js` renders all of them without error.
- Against the `.unoptimized.ll` dump beside the artifacts, the final stage of
  the `LoopSwitch` fixture agrees with codegen on phi count (18) and loop count
  (3) for the runtime object. Block counts do not match and are not expected to:
  codegen adds trap, revert and panic blocks that have no counterpart in the IR.

The one known discrepancy is a function whose exit phi count is one higher than
the dump's, which is most likely the synthesized function exit block being
materialized differently by codegen. It is worth settling during the port rather
than carrying forward.

## The schema

The manifest and stage-artifact types live in `revive_newyork::export`:
`StageManifest`, `ManifestStage`, `StageRecord`, `STAGE_MANIFEST_SCHEMA`.

[`NewYorkIrJsonFormat.md`](NewYorkIrJsonFormat.md) is the field-by-field
reference: every IR primitive and the exact JSON it serializes to.

One definition serves both directions. The compiler fills the types with borrowed
data and serializes them; `resolc-fv` deserializes them into owned data, via
`Cow` fields and an `into_owned` method that cuts a parsed value loose from the
buffer it was read out of. A second, reader-side copy of the schema would drift
from the writer's the first time a pass added a side table, and the drift would
be silent.

The manifest carries `schema: STAGE_MANIFEST_SCHEMA`, raised whenever the shape
changes. A consumer should reject a manifest whose version it does not know
rather than guess at the fields.

Note that the manifest's `yul_ast` field holds the *file name* of the dumped Yul
AST beside the manifest, not the AST itself.

## Upstream bugs

Two leaks in [`smtlib`](https://github.com/oeb25/smtlib-rs) 0.4.0, both the same
shape: a backend acquires a resource in its constructor and no `Drop` ever gives
it back. `grep -rn "impl Drop" smtlib-lowlevel-0.4.0/src/` returns nothing.

Neither is this repo's bug, and both are worth reporting upstream. A `Drop` on
each backend fixes both.

### `Z3Static` never frees its context

`src/backend/z3_static.rs` calls `Z3_mk_config` and `Z3_mk_context_rc` in
`Z3Static::new` and stores the raw context. Nothing calls `Z3_del_context` or
`Z3_del_config`, so dropping the Rust value abandons the native context.

Measured by running the probe in a loop, each iteration with a fresh solver
dropped at the end:

| probes | RSS, `z3-static` | RSS, `z3` binary |
| --- | --- | --- |
| 0 | 4 MB | 3.5 MB |
| 40 | 829 MB | 3.7 MB |
| 80 | 1594 MB | 3.7 MB |
| 120 | *process died* | 3.7 MB |

About 20 MB per solver, which a chain of any length turns into an out-of-memory
kill. This is why the solver is a separately installed binary rather than one
linked in, and why `resolc-fv` does not enable the `z3-static` feature.

### `BinaryBackend` never reaps its child

`src/backend/mod.rs:54` holds the solver process as `#[allow(unused)] child:
Child`, and nothing calls `wait()`. Dropping a `Child` in Rust detaches it
rather than reaping it. The solver does exit — closing `stdin` gives it EOF —
but it stays a zombie until `resolc-fv` itself exits.

A 108-obligation run peaked at **105 concurrent child entries**, one per probe.
They cost process-table slots rather than memory, and the operating system
clears them when the run ends, so the cost is bounded by a single run. It is a
reason to probe once per run rather than once per obligation, which would take
the peak to one.

## Open points

- No tests cover the read path yet. The one worth writing first is a round-trip:
  compile a contract with `--emit-ir-stages`, then parse every artifact back
  through the shared types. That is what catches schema drift;
  `crates/resolc/src/tests/cli/ir_stages.rs` currently only asserts the files
  appear.
- Nothing recomputes `ir_hash` against the IR it covers; parsing takes the
  manifest's hashes on trust.
- The solvers are simulated. The stubs and `simulation.rs` come out when the
  first real checker lands.
- The probe runs once per obligation. Once per run would do, and would keep the
  zombie count at one — see [Upstream bugs](#upstream-bugs).
- `resolc-fv` is not built, tested or released in CI.
- The equivalence obligation itself — what it means for two adjacent stages to
  agree, and what discharges it — is not designed yet.
