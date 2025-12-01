# `resolc` user guide

`resolc` is a Solidity `v0.8` compiler for [Polkadot `native` smart contracts](https://docs.polkadot.com/develop/smart-contracts/overview/#native-smart-contracts). Solidity compiled with `resolc` executes orders of magnitude faster than the EVM. `resolc`  also supports almost all Solidity `v0.8` features including inline assembly, offering a high level of comptability with the Ethereum Solidity reference implementation.

## `revive` vs. `resolc` nomenclature

`revive` is the name of the overarching "Solidity to PolkaVM" compiler project, which contains multiple components (for example the Yul parser, the code generation library, the `resolc` executable itself, and many more things).

`resolc` is the name of the compiler driver executable, combining many `revive` components in a single and easy to use binary application.

In other words, `revive` is the whole compiler infrastructure (more like `LLVM`) and `resolc` is a user-facing single-entrypoint compiler frontend (more like `clang`).
