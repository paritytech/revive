# CLI usage

We aim to keep the `resolc` CLI usage close to `solc`. However, due to fundamental differences of our contracts stack, there are a few things and options worthwhile to know about in `resolc` which do not exist in Ethereum. This chapter explains those.

> **Tip**
>
> For a detailed reference about the CLI, please see `resolc --help`.

### LLVM optimization levels
```bash
  -O, --optimization <OPTIMIZATION>
```

`resolc` exposes the optimization level setting for the LLVM backend. The performance and size of compiled contracts varies wiedly between different optimization levels.

Valid levels are the following:
- `0`: No optimizations are applied
- `1`: Basic optimizations for execution time.
- `2`: Advanced optimizations for execution time.
- `3`: Aggressive optimizations for execution time.
- `s`: Optimize for code size.
- `z`: Aggressively optimize for code size.

By default, `-O3` is applied.

### Stack size
```bash
      --stack-size <STACK_SIZE>
```

PVM is a register machine with a traditional stack memory space for local variables. This controls the total amount of stack space the contract can use.
          
You are incentiviced to keep this value as small as possible:
1. Increasing the heap size will increase startup costs.
2. The stack size contributes to the total memory size a contract can use, which includes the contracts code size

Default value: 65536
          
> **Warning** 
>
> If the contract uses more stack memory than configured, it will compile fine but eventually revert execution at runtime!

### Heap size
```bash
     --heap-size <HEAP_SIZE>
```

Unlike the EVM, due to the lack of dynamic memory metering, PVM contracts emulate the EVM heap memory with a static buffer. Consequentially, instead of infinite memory with exponentially growing gas costs, PVM contracts have a finite amount of memory with constant gas costs available.

You are incentiviced to keep this value as small as possible:
1.Increasing the heap size will increase startup costs.
2.The heap size contributes to the total memory size a contract can use, which includes the contracts code size

Default value: 65536
          
> **Warning**
>
> If the contract uses more heap memory than configured, it will compile fine but eventually revert execution at runtime!

### solc
```bash
      --solc <SOLC>
```

Specify the path to the `solc` executable. By default, the one in `${PATH}` is used.

### Debug artifacts
```bash
      --debug-output-dir <DEBUG_OUTPUT_DIRECTORY>
```

Dump all intermediary compiler artifacts to files in the specified directory. This includes the YUL IR, optimized and unoptimized LLVM IR, the ELF shared object and the PVM assembly. Useful for debugging and development purposes.

### Debug info
```bash
  -g
```
Generate source based debug information in the output code file. Useful for debugging and development purposes and disabled by default.

### Deploy time linking
```bash
  --link [--libraries <LIBRARIES>] <INPUT_FILES>
```

In Solidity, 3 things can happen with libraries:

1. They are not `extern`ally callable and thus can be inlined.
    1. The solc Solidity optimizer inlines those (which usually the case). Note: `resolc` always activates the solc Solidity optimizer.
    2. If the solc Solidity optimzer is disabled or for some reason fails to inline them (both rare), they are not inlined and require linking.
2. They are `extern`ally callable but still linked at compile time. This is the case if at compile time the library address is known (i.e. `--libraries` supplied in CLI or the corresponding setting in STD JSON input).
3. They are linked at deploy time. This happens when the compiler does not know the library address (i.e. `--libraries` flag is missing or the provided libraries are incomplete, same for STD JSON input). This case is rare because it's discourage and should never be used by production dApps.

In cases `1.2` and `3`:
- Some of the produced code blobs will be in the "unlinked" raw `ELF` object format and not yet deployable.
- To make them deployable, they need to be "linked" (done using the `resolc --link` linker mode explained below).
- The compiler emitted `DELEGATECALL` instructions to call non-inlined (unlinked) libraries. The contract deployer must make sure to deploy any libraries prior to contract deployment.

> **Warning**
> 
> Using deploy time linking is officially **discouraged**. Mainly due to bytecode hashes changing after the fact. We decided to support it in `resolc` regardless, due to popular reqeust.

Similar to how it works in `solc`, `--libraries` may be used to provide libraries during linking mode.

Unlike with `solc`, where linking implies a simple string substitution mechanism, `resolc` needs to resolve actual missing `ELF` symbols. This is due to how factory dependencies work in PVM. As a consequence, it isn't sufficient to just provide the unlinked blobs to the linker. Instead, they must be provided in the exact same directory structure the Solidity source code was found during compile time.

Example:
- The contract `src/foo/bar.sol:Bar` is involved in deploy time linking. It may be a factory dependency.
- The contract blob needs to be provided inside a relative `src/foo/` directory to `--link`. Otherwise symbol resolution may fail.

> **Note**
>
> Tooling is supposed to take care of this. In the future, we may append explicit linkage data to simplify the deploy time linking feature.

