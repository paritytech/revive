![CI](https://github.com/paritytech/revive/actions/workflows/rust.yml/badge.svg)
[![Docs](https://img.shields.io/badge/Docs-contracts.polkadot.io-brightgreen.svg)](https://contracts.polkadot.io)

# revive

YUL recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

Visit [contracts.polkadot.io](https://contracts.polkadot.io) to learn more about contracts on Polkadot!

## Status

This is experimental software in active development and not ready just yet for production usage. Please do report any compiler related issues or missing features that are [not yet known to us](https://contracts.polkadot.io/known_issues/) here.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation

`resolc` depends on the [solc](https://github.com/ethereum/solidity) binary installed on your system.

Download and install the `resolc` frontend executable for your platform from our [releases](https://github.com/paritytech/revive/releases).

## Building from source

Building revive requires a [stable Rust installation](https://rustup.rs/) and a C++ toolchain for building [LLVM](https://github.com/llvm/llvm-project) on your system.

### LLVM

`revive` depends on a custom build of LLVM `v18.1.8` with the RISC-V _embedded_ target, including the `compiler-rt` builtins. Use the provided [revive-llvm](crates/llvm-builder/README.md) utility to compile a compatible LLVM build locally and point `$LLVM_SYS_181_PREFIX` to the installation afterwards.

The `Makefile` provides a shortcut target to obtain a compatible LLVM build:

```bash
make install-llvm
export LLVM_SYS_181_PREFIX=${PWD}/target-llvm/gnu/target-final
```

### The `resolc` Solidity frontend

To build the `resolc` Solidity frontend executable, make sure you have obtained a compatible LLVM build using [revive-llvm](crates/llvm-builder/README.md) and did export the `LLVM_SYS_181_PREFIX` environment variable pointing to it (see [above](#LLVM)).

To install the `resolc` Solidity frontend executable:

```bash
make install-bin
resolc --version
```

### Cross-compilation to Wasm

Cross-compile the `resolc.js` frontend executable to Wasm for running it in a Node.js or browser environment. The `REVIVE_LLVM_TARGET_PREFIX` environment variable is used to control the target environment LLVM dependency.

```bash
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

### Development

Please consult the [Makefile](Makefile) targets to learn how to run tests and benchmarks. 
Ensure that your branch passes `make test` locally when submitting a pull request.

## Design overview

`revive` uses [solc](https://github.com/ethereum/solidity/), the Ethereum Solidity compiler, as the [Solidity frontend](crates/solidity/src/lib.rs) to process smart contracts written in Solidity. The YUL IR code (or legacy EVM assembly as a fallback for older `solc` versions) emitted by `solc` is then translated to LLVM IR, targetting [Polkadots `revive` pallet](https://docs.rs/pallet-revive/latest/pallet_revive/trait.SyscallDoc.html).
[Frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are based of ZKSync `zksolc`.

## Tests

Before running the tests, ensure that Geth (Go Ethereum) is installed on your system. Follow the installation guide here: [Installing Geth](https://geth.ethereum.org/docs/getting-started/installing-geth).
Once Geth is installed, you can run the tests using the following command:

```bash
make test
```
