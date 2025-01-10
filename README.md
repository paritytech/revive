![CI](https://github.com/paritytech/revive/actions/workflows/rust.yml/badge.svg)
[![Docs](https://img.shields.io/badge/Docs-contracts.polkadot.io-brightgreen.svg)](https://contracts.polkadot.io)

# revive

YUL and EVM assembly recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

Visit [contracts.polkadot.io](https://contracts.polkadot.io) to learn more about contracts on Polkadot!

## Status

This is experimental software in active development and not ready just yet for production usage. Please do report any compiler related issues or missing features that are [not yet known to us](https://contracts.polkadot.io/known_issues/) here.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation

`resolc` depends on the [solc](https://github.com/ethereum/solidity) binary installed on your system.

Building from source requires a compatible LLVM build.

### LLVM

`revive` requires a build of LLVM 18.1.4 or later with the RISC-V _embedded_ target, including `compiler-rt`. Use the provided [revive-llvm](crates/llvm-builder/README.md) utility to compile a compatible LLVM build locally (don't forget to add it to `$PATH` afterwards).

### The `resolc` Solidity frontend

To install the `resolc` Solidity frontend executable:

```bash
# Build LLVM for your default host target
make install-llvm

# Build the resolc frontend executable
export PATH=${PWD}/target-llvm/gnu/target-final/bin:$PATH
make install-bin
resolc --version
```

### Cross-compilation to WASM

Cross-compile resolc.js frontend executable to Wasm for running it in a Node.js or browser environment:

```bash
# Build LLVM for the emscripten target
cargo install --path llvm-builder
revive-llvm --target-env emscripten clone
revive-llvm --target-env emscripten build

# Build the resolc frontend executable
source emsdk/emsdk_env.sh
export LLVM_LINK_PREFIX=${PWD}/target-llvm/emscripten/target-final
export PATH=$PATH:${PWD}/target-llvm/emscripten/target-final/bin/
make install-wasm
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
