# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Revive is a Solidity compiler targeting RISC-V on PolkaVM. It uses `solc` (Ethereum Solidity compiler) as the frontend and LLVM as the backend. The main compiler binary is `resolc`.

**Compilation pipeline:**
1. `solc` lowers Solidity source to Yul IR
2. `revive` lowers Yul IR to LLVM IR
3. LLVM optimizes and emits RISC-V ELF via LLD
4. PolkaVM linker creates the final PolkaVM blob

## Build Commands

**Prerequisites:** Requires `LLVM_SYS_221_PREFIX` environment variable pointing to a compatible LLVM build. Download from paritytech/revive LLVM releases or build with `make install-llvm`.

```bash
# Install resolc binary
make install-bin

# Run all quality checks (format, clippy, machete, tests)
make test

# Individual test targets
make test-workspace      # All workspace tests (excludes llvm-builder)
make test-integration    # Integration tests (requires resolc + geth evm tool)
make test-resolc         # resolc package tests
make test-yul            # Yul parser tests

# Linting
make format              # Check formatting (cargo fmt --all --check)
make clippy              # Run clippy with --deny warnings

```

**For verifying work, claiming a task done, or pre-commit checks: only ever run
`make test*` targets — never raw `cargo test`.** Integration and resolc tests
invoke the installed `resolc` binary as a subprocess. Each `make test*` target
already declares `install-bin` (or `install`) as a dependency, so it always
rebuilds and installs a fresh binary before running tests. Raw `cargo test`
does not — it builds the test binary but leaves system `resolc` stale, so it
passes against code the test isn't actually exercising. That mismatch is
exactly why raw `cargo test` is an anti-pattern for validation.

The one legitimate exception is **actively debugging a single test** for fast
iteration: run `make install-bin` first to refresh the binary, then iterate with
`cargo test --package <crate> <test_name>`, re-running `make install-bin`
between code changes. Never use this exception for validation.

## Crate Architecture

revive compiler library crates live in `crates/` (revive Rust workspace). Tooling is not necessarily part of the workspace.


## Testing Strategy

Integration tests use **differential testing** against the Ethereum `solc`/EVM stack. Test specs are declarative JSON that define contract actions:

```json
{
    "differential": true,
    "actions": [
        {"Instantiate": {"code": {"Solidity": {"contract": "ContractName"}}}},
        {"Call": {"dest": {"Instantiated": 0}, "data": "..."}}
    ]
}
```

When `differential: true`, actions run on both EVM and PVM, asserting identical state changes.

**Running integration tests requires** the `evm` tool from go-ethereum in `$PATH`.

## Code Style Requirements

- Use `BTreeMap` instead of `HashMap` for deterministic iteration (reproducible builds)
- Avoid magic numbers/strings; use module constants
- Avoid abbreviated names; use meaningful, readable symbols
- No unnecessary macros
- Avoid import aliasing; use parent/qualified paths for conflicts
- Public items must have doc comments
- Comments should provide semantic meaning, not restate code

## Dependencies

- Add as workspace dependencies in root `Cargo.toml`
- Avoid pinning when possible
- Always include `Cargo.lock` in PRs
- Don't run `cargo update` with other changes (separate PR)
