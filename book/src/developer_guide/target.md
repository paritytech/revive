# PVM and the pallet-revive runtime target

The `revive` compiler targets [PolkaVM (PVM)](https://github.com/paritytech/polkavm) via [pallet-revive](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/revive) on Polkadot.

## Target CPU configuration

The exact target CPU configuration can be found [here](https://github.com/paritytech/revive/blob/8cd10a613625428956eb33c39c9022a91bfbf103/crates/llvm-context/src/target_machine/mod.rs#L22-L32).

> [!NOTE]
>
> The PVM linker requires fully relocatable ELF objects.

## Why PVM

PVM is a RISC-V based VM designed to overcome the flaws of [WebAssebmly (Wasm)](https://webassembly.org/). Wasm was believed to be a more efficient successor to the rather slow EVM. However, Wasm is far from an ideal target for smart contracts as some of its design decisions are unfavorable for short-lived workloads. The main problem is on-chain Wasm bytecode compilation or interpretation overhead. Prior benchmarks consistently ignoring this overhead seeded the blockchain industry with flawed assumptions: _Only when ignoring the startup overhead_ Wasm is much faster than the slow computing EVM. In practice however, gains are nullified entirely and Wasm loses completely even against very slow VMs like the EVM. Executing Wasm contracts is in fact so inefficient that typical contract workloads are _orders of magnitude_ more expensive than the equivalent EVM variant.

On the other hand, since RISC-V is similar to CPUs found in validator hardware (x86 and ARM), bytecode translation mostly boils down to a linear mapping from one instruction to another. The _embedded_ ISA specification reduces the number of general purpose registers, in turn removing the need for expensive register allocation. This guarantees single-pass `O(n)` JIT compilation of contract bytecode. The close proximity of PVM bytecode with actual validator CPU bytecode effectively allows to move all expensive compilation workload off-chain. Benchmarks ([1](https://hackmd.io/@XXX9CM1uSSCWVNFRYaSB5g/HJarTUhJA#Results-execute), [2](https://github.com/paritytech/polkavm/blob/master/BENCHMARKS.md)) show that with the PVM JIT, sandboxed PVM code executes at around half the speed of native code, which falls into the same ballpark of the state-of-the-art `wasmtime` Wasm implementation (while EVM sits somewhere around 1/10 to less than 1/100 of native speed). However, the PVM JIT compiler only uses a fraction of the time `wasmtime` requires to compile the code.

> [!NOTE]
>
> The PVM JIT isn't available yet in `pallet-revive`. At the time of writing, the contract code is interpreted, which is orders of magnitude slower than the JIT.

## Host environment: `pallet-revive`

The `revive` compiler targets the [`pallet-revive` runtime environment](https://docs.rs/pallet-revive/).

`pallet-revive` exposes a [syscall like interface](https://docs.rs/pallet-revive/latest/pallet_revive/trait.SyscallDoc.html) for contract interactions with the host environment. This is provided by the [revive-runtime-api](https://crates.io/crates/revive-runtime-api) library. 

After the initial launch on the Polkadot Asset Hub blockchain, the runtime API is considered stable and backwards compatible indefinitively.
