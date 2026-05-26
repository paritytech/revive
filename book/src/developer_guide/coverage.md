# Code coverage

Revive measures Rust line and branch coverage with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov). Coverage is
collected over the entire workspace — including the otherwise-excluded
`revive-llvm-builder` crate — so the report reflects every shipped line.

Two ways to obtain a report:

1. **Locally:** `make coverage` (described below).
2. **Per PR:** apply the `measure code coverage` label. The workflow at
   `.github/workflows/code-coverage.yml` publishes the report to
   `https://paritytech.github.io/revive/pr-<N>/` and comments the link on
   the PR. Label requires maintainer privileges.

## Running locally

```bash
make coverage
```

The target:

1. Builds and tests the workspace under cargo-llvm-cov with
   `--ignore-run-fail` so partial coverage lands even when test binaries
   exit non-zero.
2. Writes HTML to `target/coverage/html/` and a summary to
   `target/coverage/summary.txt`.
3. Stages the HTML under `book/src/coverage/`; mdbook copies it to
   `docs/coverage/` during build.
4. Rewrites the **Status** block below in place with the short commit hash,
   ISO-8601 UTC timestamp, and total line coverage, then runs
   `mdbook build`.

Expect several hours: `revive-llvm-builder` tests build LLVM end-to-end and
run single-threaded.

Prerequisites:

* `LLVM_SYS_221_PREFIX` exported and pointing at a compatible LLVM build
  (same as `make test-workspace`).
* `llvm-tools-preview` rustup component, `cargo-llvm-cov`, and the pinned
  `mdbook` version — all installed on demand by the target.

### Reverting local changes

`make coverage` modifies `book/src/developer_guide/coverage.md` in place and
stages files under `book/src/coverage/`. To restore the committed state
before pushing:

```bash
make coverage-reset
```

## Status

<!-- COVERAGE_STATUS_BEGIN -->
**Last collected:** 2026-05-26T19:00:57Z for commit `a59298e` — **38.08% line coverage**.

[Open the report](../coverage/html/index.html)
<!-- COVERAGE_STATUS_END -->
