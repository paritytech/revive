# Re-evaluating an alternative Solidity frontend

This document evaluates the feasibility of replacing `solc` as the Solidity frontend in the `revive` compiler with a Rust-native alternative.

## Motivation

The `revive` compiler currently relies on `solc` as the Solidity frontend. Solidity source code is lowered to Yul IR by calling into the `solc` executable, then `revive` parses and lowers the Yul IR to LLVM IR for RISC-V code generation. While this approach has provided a pragmatic path to high Ethereum compatibility, it carries significant drawbacks:

- **Yul is EVM-centric.** Yul is designed for a big-endian 256-bit stack machine. Our target is a 64-bit little-endian RISC-V architecture. `solc` optimizes Yul for EVM gas costs, which are orthogonal to PVM execution costs. This semantic mismatch means we inherit optimizations that are counterproductive for our target and miss optimization opportunities that neither `solc` nor LLVM can realize (see the [architecture chapter](./architecture.md)). We are currently implementing custom Yul optimization passes that reduce overall code blob sizes by 20-30%, but this approach will hit a ceiling: Yul simply does not encode enough target-relevant information to enable further gains. Fundamental inefficiencies baked into the EVM-centric IR (256-bit operations where 64-bit suffice, big-endian data layout, stack-machine idioms) cannot be optimized away at the Yul level.
- **Lost semantic information.** By the time Solidity reaches Yul, high-level semantic information (types, storage layout intent, ABI structure) has been erased. Recovering this information from Yul is either impossible or requires fragile heuristics. A higher-level IR would preserve this information, enabling better optimization decisions for PVM.
- **Slow compilation.** `solc` is notoriously slow and has problems like quadratic compilation times in the Yul optimizer. The Solidity-to-Yul lowering and Yul optimization passes in `solc` add substantial compile time, particularly for large projects.
- **External dependency.** Distributing and versioning an external `solc` binary complicates the toolchain. A Rust-native frontend would allow in-process compilation, simplify distribution, and give us full control over the pipeline.


## Evaluation of existing Rust-native Solidity frontends

Building our own frontend is expensive and not a viable path. Writing a Solidity frontend from scratch is a multi-man-year effort (see for example solang, slang, solar). Solidity has no formal language specification ([the proposal was rejected](https://github.com/ethereum/solidity/issues/12739)); the `solc` implementation is the de facto spec. This means any alternative frontend must achieve behavioral compatibility with `solc` through empirical testing, including its bugs and undocumented edge cases.

### solar (Paradigm)

[solar](https://github.com/paradigmxyz/solar) is a ground-up Solidity compiler rewrite in Rust, targeting EVM. It has a capable parser and is building semantic analysis (HIR) and codegen (MIR).

**Assessment: not viable.**

- The HIR is a moving target. Solar's internal representations are actively evolving and not designed for external consumption.
- Yul-to-HIR lowering (critical for inline assembly support) is unimplemented (issue [#415](https://github.com/paradigmxyz/solar/issues/415), marked hard, no progress).
- The MIR and codegen (PR [#693](https://github.com/paradigmxyz/solar/pull/693)) remain in draft and are EVM-specific.
- Solar's maintainers have expressed no interest in collaboration or in supporting external backend consumers.
- Development velocity on core missing features has slowed.
- Reaching out to the solar team showed that they are not interested in an non-evm backend. 

### solang (Hyperledger)

[solang](https://github.com/hyperledger-solang/solang) is a Rust Solidity compiler using LLVM, targeting Solana, Polkadot (pallet-contracts via Wasm), and Stellar. A Rust Solidity compiler using LLVM. The parser alone (`solang-parser`) took significant effort and remains at Solidity 0.8.22 compatibility. It has many bugs and inconsistent behavior against the solc reference implementation. The project has effectively stalled (no commits since November 2025, single-maintainer bus factor).

**Assessment: not viable.**

- Development has effectively stalled. No commits since November 2025.
- The semantic analysis layer is tightly coupled to codegen types (`sema/ast.rs` imports `codegen::cfg`), making frontend extraction non-trivial.
- Solidity compatibility lags behind `solc` (parser at 0.8.22, and overall unclear support and adoption rate for Solidity features).

### slang (Nomic Foundation)

[slang](https://github.com/NomicFoundation/slang) is a modular Solidity compiler tooling library by the Nomic Foundation (the Hardhat team). Reaching a stable v1.0 parser took over 1,500 pull requests and years of development by a well-funded team. The semantic analysis backend remains behind a private, unstable API.

**Assessment: viable and recommended.**

- The parser API reached **v1.0 in March 2025** and is now at v1.3.3 with stable semver guarantees. It supports Solidity versions 0.4.11 through 0.8.34.
- Error-tolerant parsing produces full-fidelity concrete syntax trees (CSTs).
- Name resolution via the binding graph is part of the public, stable API.
- Type information is being progressively exposed through the public API.
- Actively maintained by a well-funded team (Nomic Foundation), with consistent releases.
- MIT licensed.
- Most importantly: slang has been **validated as a compiler frontend** by the [solx](https://github.com/NomicFoundation/solx) project (see below).

## solx: validation of the slang frontend approach

[solx](https://github.com/NomicFoundation/solx) is an LLVM-based optimizing Solidity compiler targeting EVM, developed jointly by Matter Labs and Nomic Foundation. It is architecturally similar to `revive`: it uses `solc` as its current frontend, lowers through LLVM, and targets a specific bytecode format.

solx is actively building a **second frontend based on slang** ([PR #219](https://github.com/NomicFoundation/solx/pull/219), merged February 2026). The pipeline is:

```text
Solidity source --> slang (parse) --> MLIR (LLVM dialect) --> LLVM IR --> EVM bytecode
```

Key observations from solx's slang integration:

- The `solx-slang` crate uses the stable `slang_solidity` v1.3 parser API. No private/unstable APIs are used.
- An `solx-mlir` crate provides the MLIR-to-LLVM-IR translation layer using the `melior` crate (Rust MLIR bindings).
- The Solidity-to-MLIR lowering is under active development (currently a stub, but plumbing is complete).
- The `solx-slang` and `solx-mlir` crates are **MIT/Apache-2.0 licensed**, making them reusable.

solx's adoption of slang provides strong evidence that slang is mature enough to serve as a compiler frontend. It also establishes MLIR as a viable intermediate layer between parsing and LLVM IR generation.

## Proposed direction

We propose adopting slang as the Solidity frontend, translating slang's output to a custom Rust-native IR, and lowering through LLVM to PVM. The target pipeline is:

```text
Solidity ──► slang (parse, resolve) ──► custom IR ──► [Optimizations] ──► LLVM IR ──► PVM
                                        ▲
              inline assembly { } ──► from_yul (existing Yul lowering)
```

This pipeline replaces both `solc` and Yul, giving us:

- **Rust-native, in-process compilation.** No external `solc` binary.
- **High-level semantic information preserved.** The IR can retain type, storage layout, and ABI information that Yul discards, enabling PVM-specific optimizations beyond what is achievable at the Yul level.
- **Target-appropriate IR.** The IR and lowering passes can be designed for our 64-bit RISC-V target from the start, rather than fighting EVM-centric 256-bit idioms.
- **Faster compilation.** slang's parser is significantly faster than `solc`, and eliminating the `solc` subprocess and Yul round-trip removes overhead.

### Inline assembly (Yul) handling

Solidity contracts may contain inline `assembly { }` blocks written in Yul. slang parses these blocks into Yul AST nodes. For inline assembly, we extract the parsed Yul and route it through `revive`'s existing Yul-to-IR translation. The new frontend handles Solidity-level semantics natively, while inline assembly falls back to the proven Yul pipeline.

### Metadata, ABI, and storage layout

Building a slang-based frontend inherently requires type resolution, C3 inheritance linearization, and full semantic analysis — these are not optional extras but prerequisites for correct code generation. Storage layout computation, ABI generation, and NatSpec extraction are natural byproducts of this same infrastructure.

**Storage layout.**

slang already has storage layout computation behind its `__private_backend_api` feature flag (~200-300 lines of Rust). It handles packing, structs, fixed/dynamic arrays, mappings, and inheritance via C3 linearization. Known gaps: no `layout at` (Solidity 0.8.29+), no transient storage, a struct packing edge case, and no differential testing against `solc` ([issue #1517](https://github.com/NomicFoundation/slang/issues/1517)).

We use slang's implementation directly by enabling the `__private_backend_api` feature flag and pinning our slang dependency version. The `__private_` prefix is a semver convention (the API may change without a major version bump), not a technical access restriction — the code is open source and MIT licensed. By pinning the version and updating deliberately, we control when breaking changes affect us. We contribute fixes for the known gaps upstream and add differential testing against `solc` ourselves.

Getting storage layout wrong has severe consequences: broken proxy upgrades (ERC-1967, ERC-7201), corrupt on-chain state, and incompatible cross-contract calls. Our validation strategy:

1. **Test-time verification.** For every contract in our test suite, we compare slang's computed storage layout against `solc`'s `storageLayout` JSON output (available via `outputSelection: {"*": {"*": ["storageLayout"]}}`). Any divergence is a bug to be fixed in slang or reported upstream.
2. **Runtime differential testing.** Our existing test infrastructure deploys the same contract on both EVM (via `solc`) and PVM (via our frontend), executes identical transactions, and performs slot-by-slot storage comparison. A layout bug manifests as a state divergence.

**ABI and NatSpec.**

ABI generation follows a well-defined specification and is derived from the same type resolution infrastructure. NatSpec extraction (devdoc/userdoc) requires parsing `@dev`, `@notice`, `@param`, `@return` tags from comments attached to declarations — slang preserves these as CST trivia nodes. Both are tractable engineering work.

**Contract metadata hash.** Generated by us. This CBOR-encoded blob identifies the compiler that produced the bytecode (version, settings, source hashes). Since we are a different compiler, we emit our own metadata hash rather than claiming to be `solc`.

### Why a custom Rust IR, not MLIR

solx chose [MLIR](https://mlir.llvm.org/) as its intermediate layer between slang and LLVM. We choose a custom Rust-native IR instead:

- **No additional C++ FFI surface.** MLIR's Rust bindings (`melior`) are thin wrappers around C APIs. Debugging across Rust/C FFI/MLIR/LLVM boundaries is significantly harder than debugging pure Rust. A custom IR is fully debuggable with standard Rust tooling.
- **Natural egglog integration.** We plan to use [egglog](https://github.com/egraphs-good/egglog)-based equality saturation for optimization (see below). egglog is Rust-native; custom IR nodes map directly to egglog terms without serialization overhead. MLIR + egglog is feasible — [Numba v2](https://proceedings.scipy.org/articles/fncj2446) and [DialEgg](https://dl.acm.org/doi/10.1145/3696443.3708957) (CGO 2025) demonstrate this — but in a Rust codebase the FFI round-trips between Rust, MLIR's C API, and egglog add friction that a custom IR avoids.
- **MLIR is a framework, not a solution.** MLIR provides infrastructure for defining IRs (dialects, passes, verifiers) but does not provide domain-specific optimizations out of the box. We would still need to implement all PVM-specific and Solidity-specific optimization passes ourselves — MLIR just changes the language we write them in (C++/tablegen instead of Rust). The value proposition of MLIR is strongest when reusing existing community dialects; in our niche domain (Solidity-to-RISC-V), there are none to reuse.
- **No shared dialect benefit.** solx's MLIR content will be EVM-shaped (256-bit values, EVM address spaces, EVM intrinsics). Sharing dialects would require target parameterization that neither project has built.
- **Precedent.** solar's MIR, Fe's Sonatina, and Cranelift all chose custom Rust-native IRs over MLIR.
- **No risk**. In the future, we can switch to an MLIR based pipeline any time if necessary.

### Optimization via equality saturation (egglog)

The hand-written sequential optimization passes common in compilers suffer from pass ordering sensitivity: local rewrites can miss global optima. [egglog](https://github.com/egraphs-good/egglog) (Datalog-powered equality saturation over e-graphs) offers a principled alternative.

This is particularly attractive for our use case: many Solidity idioms that are efficient on EVM (256-bit operations, big-endian memory access patterns, stack-manipulation tricks) have cheaper PVM-native equivalents. Equality saturation can discover these substitutions holistically:

- Define IR nodes as egglog terms
- Express optimization rules as equivalences (e.g., `i256.add(x, y)` where both operands fit in i64 is equivalent to `i64.zext(i64.add(i64.trunc(x), i64.trunc(y)))`)
- Define a PVM-specific cost model (instruction cost, code size, byte-swap overhead)
- Let the engine explore the full space of equivalent programs and extract the optimal variant

Precedent for e-graph-based compiler optimization exists in [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) (Wasmtime's compiler backend), which uses e-graphs on its own Rust-native IR.

### Extending the IR for Solidity-level semantics

With slang as the frontend, the IR can be extended beyond Yul-level constructs:

- **ABI encode/decode as first-class operations.** Currently expanded by `solc` into verbose mstore/mload sequences. A single IR node enables specialized lowering (e.g., direct memcpy for static types, skip padding for PVM-native layouts).
- **Storage layout annotations.** Slot indices, mapping key structures, and array base pointers as IR metadata enable storage access pattern optimization.
- **Contract dispatch.** Function selector matching as a structured IR construct rather than a chain of `if/eq` comparisons enables jump-table lowering.
- **Inheritance linearization.** Resolved by slang's binding graph, enabling cross-function optimization across inherited methods.

## Next steps

1. **Prototype the slang-to-IR translation** for a minimal contract (storage read/write, single function, no inheritance). Validate that slang's CST and binding graph provide sufficient information to populate the IR.
2. **Engage with the solx team** to share learnings on slang integration, particularly around Solidity semantic lowering and the Yul inline assembly boundary.
3. **Incrementally expand the translation** to cover Solidity features: function dispatch, inheritance, ABI encoding, modifiers, events, error handling.
4. **Add egglog-based optimization** for type narrowing and algebraic simplification as a proof of concept.
5. **Maintain the existing `solc`/Yul pipeline** as the production path while the slang frontend matures. Both paths coexist via feature flags (similar to how solx supports `--features solc` and `--features slang`).
