# Compiler architecture

`revive` relies on `solc`, the [Ethereum Solidity compiler](https://github.com/ethereum/solidity), as the Solidity frontend to process smart contracts written in Solidity. [LLVM](https://github.com/llvm/llvm-project), a popular and powerful compiler framework, is used as the compiler backend and does the heavy lifting in terms of optimizitations and RISC-V code generation.

`revive` mainly takes care of lowering the YUL intermediate representation (IR) produced by `solc` to LLVM IR. This approach provides a good balance between maintaining a high level of Ethereum compatibility, good contract performance and feasible engineering efforts.

## `resolc`

### Compilation process

When compiling a Solidity source file, the following steps happen under the hood:
1. `solc` is used to lower the Solidity source code into [YUL intermediate representation](https://docs.soliditylang.org/en/latest/yul.html).
2. `revive` lowers the YUL IR into LLVM IR.
3. LLVM optimizes the code and emits a RISC-V ELF shared object (through LLD).
4. The [PolkaVM](https://github.com/paritytech/polkavm) linker finally links the ELF shared object into a PolkaVM blob.

This compilation process can be visualized as follows:

![Architecture Overview](images/resolc.svg)

### Single static binary distribution

## The `revive` compiler libraries

The main compiler logic is implemented in the `revive-yul` and `revive-llvm-context` crates.

The Yul library implements a lexer and parser and lowers the resulting tree into LLVM IR. It does so by emitting LL using the LLVM builder and also our own `revive-llvm-context` compiler context crate. The revive LLVM context crate encapsulates code generation logic (decoupled from the parser).

## EVM heap memory

PVM doesn't offer a similar API. Hence the emitted contract code emulates the linear EVM heap memory using a static byte buffer. Data inside this byte buffer is kept big endian for EVM compatibility reasons (unaligned access is allowed and makes optimizing this non-trivial).

Unlike with the EVM, where heap memory usage is gas metered, our heap size is static (the size is user controllable via a setting flag). The compiler emits bound checks to prevent overflows.

## The LLVM dependency

LLVM is a special non Rust dependency. We interface its builder interface via the [inkwell](https://crates.io/crates/inkwell) wrapper crate.

We use upstream LLVM but release and use our custom builds. We require the compiler builtins specifically built for the PVM rv64e target and leave assertions always on. Further we need cross builds because `resolc` itself targets emscripten and musl. The [revive-llvm-builer](https://crates.io/crates/revive-llvm-builder) functions as a cross-plattform build script and is used to build and release the LLVM dependency.

We also maintain the [lld-sys crate](https://crates.io/crates/lld-sys) for interfacing with `LLD`. The LLVM linker is used during the compilation process but we don't want to distribute another binary.


## Custom optimizations

At the moment, no real custom optimizations are implemented. Thus we are missing some optimization opportunities that neither solc nor LLVM can realize (due to their lack of domain specific semantics). Further, solc optimizes for target machine orthogonal to our target (BE 256bit stack machine vs. 64bit LE RISC architecture) and the EVM gas model. We started working on an additional IR layer between Yul and LLVM to capture this.
