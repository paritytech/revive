![CI](https://github.com/paritytech/revive/actions/workflows/test.yml/badge.svg)
[![Docs](https://img.shields.io/badge/Docs-contracts.polkadot.io-brightgreen.svg)](https://contracts.polkadot.io/revive_compiler/)

# revive

YUL recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

Visit [contracts.polkadot.io](https://contracts.polkadot.io) to learn more about contracts on Polkadot!

## Status

This is experimental software in active development and not ready just yet for production usage. Please do report any compiler related issues or missing features that are [not yet known to us](https://contracts.polkadot.io/known_issues/) here.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation

Please consult [the documentation](https://contracts.polkadot.io/revive_compiler/installation) for installation instructions.

## Building from source

Building revive requires a [stable Rust installation](https://rustup.rs/) and a C++ toolchain for building [LLVM](https://github.com/llvm/llvm-project) on your system.

### LLVM

`revive` depends on a custom build of LLVM `v18.1.8` with the RISC-V _embedded_ target, including the `compiler-rt` builtins. You can either download a build from our releases (recommended for older hardware) or build it from source.

<details>
  <summary>Download from our LLVM releases</summary>

Download the [latest LLVM build](https://github.com/paritytech/revive/releases?q=LLVM+binaries+release&expanded=true) from our releases.

> **MacOS** users need to clear the `downloaded` attribute from all binaries after extracting the archive:
>
> ```sh
> xattr -rc </path/to/the/extracted/archive>/target-llvm/gnu/target-final/bin/*
> ```

After extracting the archive, point `$LLVM_SYS_181_PREFIX` to it:

```sh
export LLVM_SYS_181_PREFIX=</path/to/the/extracted/archive>/target-llvm/gnu/target-final
```

</details>

<details>
  <summary>Building from source</summary>

Use the provided [revive-llvm](crates/llvm-builder/README.md) utility to compile a compatible LLVM build locally and point `$LLVM_SYS_181_PREFIX` to the installation afterwards.

The `Makefile` provides a shortcut target to obtain a compatible LLVM build:

```sh
make install-llvm
export LLVM_SYS_181_PREFIX=${PWD}/target-llvm/gnu/target-final
```

</details>

### The `resolc` Solidity frontend

To build the `resolc` Solidity frontend executable, make sure you have obtained a compatible LLVM build and did export the `LLVM_SYS_181_PREFIX` environment variable pointing to it (see [above](#LLVM)).

To install the `resolc` Solidity frontend executable:

```sh
make install-bin
resolc --version
```

### Cross-compilation to Wasm

Cross-compile the `resolc.js` frontend executable to Wasm for running it in a Node.js or browser environment. The `REVIVE_LLVM_TARGET_PREFIX` environment variable is used to control the target environment LLVM dependency.

<details>
  <summary>Instructions for cross-compilation to wasm32-unknown-emscripten</summary>

```sh
# Build the host LLVM dependency with PolkaVM target support
make install-llvm
export LLVM_SYS_181_PREFIX=${PWD}/target-llvm/gnu/target-final

# Build the target LLVM dependency with PolkaVM target support
revive-llvm --target-env emscripten clone
source emsdk/emsdk_env.sh
revive-llvm --target-env emscripten build --llvm-projects lld
export REVIVE_LLVM_TARGET_PREFIX=${PWD}/target-llvm/emscripten/target-final

# Build the resolc frontend executable
make install-wasm
make test-wasm
```

</details>

## Development

Please consult the [Makefile](Makefile) targets to learn how to run tests and benchmarks.
Ensure that your branch passes `make test` locally when submitting a pull request.

### Design overview

See the [relevant section in our documentation](https://contracts.polkadot.io/revive_compiler/architecture) to learn more about how the compiler works.

[Frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are based of ZKSync `zksolc` (the project started as a fork of the era compiler).

### Tests

Before running the tests, ensure that Geth (Go Ethereum) is installed on your system. Follow the installation guide here: [Installing Geth](https://geth.ethereum.org/docs/getting-started/installing-geth).
Once Geth is installed, you can run the tests using the following command:

```sh
make test
```
