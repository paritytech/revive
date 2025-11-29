# `resolc` user guide

`resolc` is a Solidity compiler for [Polkadot `native` smart contracts](https://docs.polkadot.com/develop/smart-contracts/overview/#native-smart-contracts). Quoting from the linked contract docs:

> Developers can utilize familiar Ethereum libraries for contract interactions and leverage industry-standard development environments for writing and testing smart contracts.

The `resolc` compiler implements efficient Solidity support on Polkadot native contracts by compiling Solidity source to PVM.

## `revive` vs. `resolc` nomenclature

`revive` is the name of the overarching "Solidity to PolkaVM" compiler project, which contains multiple components (for example the Yul parser, the code generation library, the `resolc` executable itself and many things more).

`resolc` is the name of the compiler driver executable, combining many `revive` components in a single and easy to use binary application.

In other words, `revive` is the whole compiler infrastructure (more like `LLVM`) and `resolc` is a user-facing single-entrypoint compiler frontend (more like `clang`).
