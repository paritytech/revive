![CI](https://github.com/paritytech/revive/actions/workflows/test.yml/badge.svg)
[![Docs](https://img.shields.io/badge/Docs-contracts.polkadot.io-brightgreen.svg)](https://contracts.polkadot.io/revive_compiler/)

# revive

YUL recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

Visit [contracts.polkadot.io](https://contracts.polkadot.io) to learn more about contracts on Polkadot!

## Status

This is experimental software in active development and not ready just yet for production usage. Please do report any compiler related issues or missing features that are [not yet known to us](https://contracts.polkadot.io/known_issues/) here.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation
Building Solidity contracts for PolkaVM requires installing the following two compilers:
- `resolc`: The revive Solidity compiler YUL frontend and PolkaVM code generator (provided by this repository).
- `solc`: The [Ethereum Solidity reference compiler](https://github.com/ethereum/solidity/) implemenation.`resolc` uses `solc` during the compilation process, please refer to the [Ethereum Solidity documentation](https://docs.soliditylang.org/en/latest/installing-solidity.html) for installation instructions.

### `resolc` binary releases
`resolc` is distributed as a standalone binary (with `solc` as the only external dependency). Please download one of our [binary releases](https://github.com/paritytech/revive/releases) for your target platform and mind the platform specific instructions below.

<details>
  <summary>MacOS users</summary>

> **MacOS** users need to clear the `downloaded` attribute from the binary and set the executable flag.
> ```sh
> xattr -rc resolc-universal-apple-darwin
> chmod +x resolc-universal-apple-darwin
> ```

</details>

<details>
  <summary>Linux users</summary>

> **Linux** users need to set the executable flag.
> ```sh
> chmod +x resolc-x86_64-unknown-linux-musl
> ```

</details>


### `resolc` NPM package
We distribute the revive compiler as [node.js module](https://www.npmjs.com/package/@parity/resolc) and [hardhat plugin](https://www.npmjs.com/package/@parity/hardhat-polkadot-resolc).

Note: The `solc` dependency is bundled via NPM packaging and defaults to the latest supported version.

## Building from source

Building revive requires a [stable Rust installation](https://rustup.rs/) and a C++ toolchain for building [LLVM](https://github.com/llvm/llvm-project) on your system.

### LLVM

`revive` depends on a custom build of LLVM `v18.1.8` with the RISC-V _embedded_ target, including the `compiler-rt` builtins. You can either download a build from our releases (recommended for older hardware) or build it from source.

<details>
  <summary>Download from our LLVM releases</summary>

Download the [latest LLVM build](https://github.com/paritytech/revive/releases?q=LLVM+binaries+release&expanded=true) from our releases.

> **MacOS** users need to clear the `downloaded` attribute from all binaries after extracting the archive:
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

The `Makefile` provides a shortcut target to obtain a compatible LLVM build, using the provided [revive-llvm](crates/llvm-builder/README.md) utility. Once installed, point `$LLVM_SYS_181_PREFIX` to the installation afterwards:

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

### Tests

Before running the tests, ensure that Geth (Go Ethereum) is installed on your system. Follow the installation guide here: [Installing Geth](https://geth.ethereum.org/docs/getting-started/installing-geth).
Once Geth is installed, you can run the tests using the following command:

```sh
make test
```
# Acknowledgements

The revive compiler project, after some early experiments with EVM bytecode translations, decided to fork the `era-compiler` framework.
[Frontend](https://github.com/matter-labs/era-compiler-solidity), [code generator](https://github.com/matter-labs/era-compiler-llvm-context) and some supporting libraries are based of ZKSync `zksolc`. I'd like to express my gratitude and thank the original authors for providing a useable code base under a generous license.

