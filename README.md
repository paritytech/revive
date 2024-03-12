# revive

YUL and EVM bytecode recompiler to LLVM, targetting RISC-V on PolkaVM.

Code bases of [frontend](https://github.com/matter-labs/era-compiler-solidity) and [code generator](https://github.com/matter-labs/era-compiler-llvm-context) are forked adapted from ZKSync `zksolc`.

Primary goal of this codebase currently is to allow for benchmarks comparing runtime performance against ink!, solang and EVM interpreters.

# TODO

The project is in a very early PoC phase; at this stage don't expect the produced code to be working nor to be correct for anything more than a basic flipper contract yet.

- [ ] Efficient implementations of byte swaps, memset, memmove and the like
- [ ] Use drink! for integration tests once we have 64bit support in PolkaVM
- [ ] Exercice `schlau` and possibly `smart-bench` benchmark cases
- [ ] Tests currently rely on the binary being in $PATH, which is very annoying and requires `cargo install` all the times
- [ ] Define how to do deployments
- [ ] Calling conventions for calling other contracts
- [ ] Runtime environment isn't fully figured out; implement all EVM builtins
- [ ] Iron out many leftovers from the ZKVM target
    - [ ] Use of exceptions
    - [ ] Change long calls (contract calls)
    - [ ] Check all alignments, attributes etc. if they still make sense with our target
- [ ] Add a lot more test cases
- [ ] Debug information
- [ ] Look for and implement further optimizations
- [ ] Differential testing against EVM
- [ ] Switch to LLVM 18 which has RV{32,64}E upstream
- [ ] Minimize scope of "stdlib", favorably implement it in high level language instead of LLVM IR.
- [ ] Document differences from EVM
- [ ] Audit for bugs and correctness
- [ ] Rebranding
