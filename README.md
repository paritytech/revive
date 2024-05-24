![CI](https://github.com/xermicus/revive/actions/workflows/rust.yml/badge.svg)

# revive

YUL and EVM assembly recompiler to LLVM, targetting RISC-V on [PolkaVM](https://github.com/koute/polkavm).

[Frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are based of ZKSync `zksolc`.

## Status

This is experimental software in active development and not ready just yet for production usage.

Discussion around the development is hosted on the [Polkadot Forum](https://forum.polkadot.network/t/contracts-update-solidity-on-polkavm/6949#a-new-solidity-compiler-1).

## Installation

TL;DR installing the `resolc` Solidity frontend executable:

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

## Design overview
`revive` uses [solc](https://github.com/ethereum/solidity/), the Ethereum Solidity compiler, as the [Solidity frontend](crates/solidity/src/lib.rs) to process smart contracts written in Solidity. The YUL IR code (or legacy EVM assembly as a fallback for older `solc` versions) emitted by `solc` is then translated to LLVM IR, targetting a runtime similar to [Polkadots `contracts` pallet](https://docs.rs/pallet-contracts/latest/pallet_contracts/api_doc/trait.Current.html).

## TODO

- [ ] Efficient implementations of byte swaps, memset, memmove, mulmod and the like
- [ ] Migrate the mock runtime to the on-chain implementation when its avaialbe
- [x] Use PolkaVM allocator for heap space
- [ ] Exercice `schlau` and possibly `smart-bench` benchmark cases
- [x] Tests currently rely on the binary being in $PATH, which is very annoying and requires `cargo install` all the times
- [x] Define how to do deployments
- [ ] Calling conventions for calling other contracts
- [ ] Runtime environment isn't fully figured out; implement all EVM builtins
- [ ] Iron out leftovers from the ZKVM target
    - [ ] Use of exceptions
    - [ ] Change long calls (contract calls)
    - [ ] Check all alignments, attributes etc. if they still make sense with our target
    - [x] Custom extensions related to zk VM
    - [x] `Active Pointer`: Redundant to calldata forwarding in pallet contracts. [Mainly used here](https://github.com/matter-labs/era-contracts/blob/4aa7006153ad571643342dff22c16eaf4a70fdc1/system-contracts/contracts/libraries/EfficientCall.sol) however we could offer a similar optimization.
- [ ] Add a lot more test cases
- [ ] Debug information
- [ ] Look for and implement further optimizations
- [ ] Differential testing against EVM
- [x] Switch to LLVM 18 which has `RV{32,64}E` targets upstream
- [ ] Minimize scope of "stdlib"
- [ ] Document differences from EVM
- [ ] Audit for bugs and correctness
- [x] Rebranding
- [ ] Remove support for vyper
