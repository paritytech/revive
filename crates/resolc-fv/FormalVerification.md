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

**Status: scaffold.** The binary builds and prints a banner. It does not read any
artifacts yet.

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

## Open points

- No tests cover the read path yet. The one worth writing first is a round-trip:
  compile a contract with `--emit-ir-stages`, then parse every artifact back
  through the shared types. That is what catches schema drift;
  `crates/resolc/src/tests/cli/ir_stages.rs` currently only asserts the files
  appear.
- `resolc-fv` is not built, tested or released in CI.
- The equivalence obligation itself — what it means for two adjacent stages to
  agree, and what discharges it — is not designed yet.
