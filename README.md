# revive

YUL and EVM bytecode recompiler to LLVM, targetting RISC-V on PolkaVM.

Code bases of [frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are forked and adapted from ZKSync `zksolc`.

# Status

Currently, primary goal of this codebase is to allow for benchmarks comparing performance against ink! and solang artifacts as well as EVM interpreters.

# TODO

The project is in a very early PoC phase. Don't yet expect the produced code to be working nor to be correct for anything more than a basic flipper contract at the current stage.

- [ ] Efficient implementations of byte swaps, memset, memmove, mulmod and the like
- [ ] Use `drink` for integration tests once we have 64bit support in PolkaVM
- [x] Use PolkaVM allocator for heap space
- [ ] Exercice `schlau` and possibly `smart-bench` benchmark cases
- [x] Tests currently rely on the binary being in $PATH, which is very annoying and requires `cargo install` all the times
- [x] Define how to do deployments
- [ ] Calling conventions for calling other contracts
- [ ] Runtime environment isn't fully figured out; implement all EVM builtins
- [ ] Iron out many leftovers from the ZKVM target
    - [ ] Use of exceptions
    - [ ] Change long calls (contract calls)
    - [ ] Check all alignments, attributes etc. if they still make sense with our target
    - [x] Custom extensions related to zk VM
    - [ ] `Active Pointer`: Redundant to calldata forwarding in pallet contracts. [Mainly used here](https://github.com/matter-labs/era-contracts/blob/4aa7006153ad571643342dff22c16eaf4a70fdc1/system-contracts/contracts/libraries/EfficientCall.sol) however we could offer a similar optimization.
    - []
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
