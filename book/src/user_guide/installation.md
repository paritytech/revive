# Installation

Building Solidity contracts for PolkaVM requires installing the following two compilers:
- `solc`: The [Ethereum Solidity reference compiler](https://github.com/ethereum/solidity/) implemenation. 
- `resolc`: The revive Solidity compiler YUL frontend and PolkaVM code generator.

## Dependencies

`resolc` uses `solc` during the compilation process, please refer to the [Ethereum Solidity documentation](https://docs.soliditylang.org/en/latest/installing-solidity.html) for installation instructions.

## `resolc` binary releases

`resolc`  is supported an all major operating systems and installation is straight forward.
Please find our [binary releases](https://github.com/paritytech/revive/releases) for the following platforms:
- Linux (MUSL)
- MacOS (uinversal)
- Windows
- Wasm via emscripten

## `revive` NPM package

We distribute the revive compiler as [node.js module](https://github.com/paritytech/js-revive).

## Buidling `resolc` from source

Please follow the build [instructions in the revive `README.md`](https://github.com/paritytech/revive?tab=readme-ov-file#building-from-source).

