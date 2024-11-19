![CI](https://github.com/paritytech/revive/actions/workflows/rust.yml/badge.svg)
[![Docs](https://img.shields.io/badge/Docs-contracts.polkadot.io-brightgreen.svg)](https://contracts.polakdot.io)

# revive

YUL and EVM assembly recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

Visit [contracts.polkadot.io](contracts.polkadot.io) to learn more about contracts on Polkadot!

## Status

This is experimental software in active development and not ready just yet for production usage. Please do report any compiler related issues or missing features that are [not yet known to us](https://contracts.polkadot.io/known_issues/) here.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation

`resolc` depends on the [solc](https://github.com/ethereum/solidity) binary installed on your system.

To install the `resolc` Solidity frontend executable:

```bash
bash build-llvm.sh
export PATH=${PWD}/llvm18.0/bin:$PATH
make install-bin
resolc --version
```

### LLVM

`revive` requires a build of LLVM 18.1.4 or later including `compiler-rt`. Use the provided [build-llvm.sh](build-llvm.sh) build script to compile a compatible LLVM build locally in `$PWD/llvm18.0` (don't forget to add that to `$PATH` afterwards). 

### Development

Please consult the [Makefile](Makefile) targets to learn how to run tests and benchmarks. 
Ensure that your branch passes `make test` locally when submitting a pull request.

## Design overview
`revive` uses [solc](https://github.com/ethereum/solidity/), the Ethereum Solidity compiler, as the [Solidity frontend](crates/solidity/src/lib.rs) to process smart contracts written in Solidity. The YUL IR code (or legacy EVM assembly as a fallback for older `solc` versions) emitted by `solc` is then translated to LLVM IR, targetting [Polkadots `revive` pallet](https://docs.rs/pallet-revive/latest/pallet_revive/trait.SyscallDoc.html).

[Frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are based of ZKSync `zksolc`.
