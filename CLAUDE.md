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

**Prerequisites:** Requires `LLVM_SYS_181_PREFIX` environment variable pointing to a compatible LLVM 18.1.8 build. Download from releases or build with `make install-llvm`.

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

# Run single test
cargo test --package <crate-name> <test_name>
cargo test --package revive-integration <test_name>
```

## Crate Architecture

All crates live in `crates/`. Key crates:

- **resolc** - Compiler driver binary and library; orchestrates the full pipeline
- **revive-yul** - Yul lexer, parser, and LLVM IR lowering; implements visitor pattern for AST traversal
- **revive-llvm-context** - LLVM code generation logic (decoupled from parser)
- **revive-linker** - Links RISC-V ELF to PolkaVM blob via LLD and polkavm-linker
- **revive-runner** - Executes contracts in simulated pallet-revive runtime; provides declarative test spec format
- **revive-integration** - Integration test cases using revive-runner
- **revive-differential** - Differential testing utilities against EVM
- **revive-runtime-api** - Low-level runtime API bindings to pallet-revive
- **revive-stdlib** - Compiler standard library components
- **revive-common** - Shared constants and utilities
- **lld-sys** - FFI bindings to LLVM's LLD linker

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

## Rust Version

Minimum supported: 1.85.0 (specified in workspace)
