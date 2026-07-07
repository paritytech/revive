# Code coverage

Revive measures Rust line and branch coverage with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) over the same
crate set as `make test-workspace` (the workspace minus
`revive-llvm-builder`).

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
2. Writes HTML to `book/src/coverage/html/` and a summary to
   `book/src/coverage/summary.txt`.
3. Runs `mdbook build`, which copies the report to `docs/coverage/`.

Everything it writes lives under `book/src/coverage/` and `docs/coverage/`,
both of which are gitignored, so a coverage run never touches tracked files —
there is nothing to revert before committing.

Open the report at `docs/coverage/html/index.html`, or run `make book` to
browse it in the rendered book.

Prerequisites:

* `LLVM_SYS_221_PREFIX` exported and pointing at a compatible LLVM build
  (same as `make test-workspace`).
* `llvm-tools-preview` rustup component, `cargo-llvm-cov`, and the pinned
  `mdbook` version — all installed on demand by the target.
