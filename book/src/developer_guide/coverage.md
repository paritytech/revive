# Code coverage

Revive measures Rust and LLVM C++ line and region coverage with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) over the same tests as `make test-workspace`.

Two ways to obtain a report:

1. **Locally:** `make coverage` and `make coverage-llvm-report` (described below).
2. **Per PR:** apply the `measure code coverage` label and read the bot-generated PR comment once the triggered workflow completes.

## Running locally

```bash
# Install LLVM.
make install-llvm

# Or install instrumented LLVM.
make install-llvm-coverage

# ..set LLVM_SYS_221_PREFIX to a compatible LLVM build..

# Run coverage and generate revive Rust report.
make coverage

# Generate a separate LLVM C++ report if needed, if
# instrumented LLVM was used for `make coverage`.
make coverage-llvm-report

# Print clickable links to all accumulated HTML reports detected
# (they're grouped by timestamps) to easily view them in the browser.
make coverage-browse
```

The target:

1. Builds and tests the workspace under cargo-llvm-cov with `--ignore-run-fail` so partial coverage lands even when test binaries exit non-zero.
2. Writes a browsable HTML report into a timestamped run directory (`coverage-reports/<stamp>/revive/` or `coverage-reports/<stamp>/llvm/`).

## Resource requirements and troubleshooting

`make coverage` bakes in two defaults that keep instrumented runs tractable:
* It drops DWARF with `CARGO_PROFILE_DEV_DEBUG=0` since coverage line data lives in the binaries' `__llvm_covmap` sections.
* It bounds profile output with `LLVM_PROFILE_FILE_NAME=revive-%4m.profraw`. The default pattern contains `%p`, writing one raw profile per process. With an instrumented LLVM this could exceed tens of gigabytes of raw profiles in a full run. The `%4m` merge pool makes processes merge their counters into a handful of files instead.

Even with those defaults, plan for:

* Disk space:
  * Building instrumented LLVM and running an instrumented `make coverage` still needs tens of gigabytes of disk space.
* LLVM link memory (`make install-llvm-coverage`):
  * If a link step gets OOM-killed, rerun with capped parallelism (e.g. `JOBS=1`) to serialize the links. The rerun resumes incrementally.
* Test-binary link memory (`make coverage`):
  * If a link gets OOM-killed, rerun with `RUSTFLAGS="-C link-arg=-fuse-ld=lld"` (or _additionally_ with capped parallelism, e.g. `CARGO_BUILD_JOBS=1`). GNU ld holds every input object and the coverage mapping of the entire dependency closure in memory at once. lld links with a smaller peak.
* Test-run memory (`make coverage`):
  * Each concurrently running test spawns an instrumented resolc. On memory-constrained machines cap it with `RUST_TEST_THREADS=4`. This throttles test execution only, not compilation.
