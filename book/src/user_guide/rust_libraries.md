# Rust contract libraries

> [!NOTE]
>
> This is not yet implemented but something for consideration on the roadmap.

Solidity - tightly coupled to the EVM - introduces some inherent inefficiencies that are by design and either needs to be followed or can't be easily worked around, even with efforts like better optimized compiler and VM implementations. This represents a technical dead end. So far the EVM sees no adoption beyond the blockchain industry. Chances are that [the EVM end up deprecated](https://ethereum-magicians.org/t/long-term-l1-execution-layer-proposal-replace-the-evm-with-risc-v) for technical reasons (or maybe not and the RISC-V idea gets abandoned, who knows).

PVM, however, is a general purpose VM. It supports LLVM based mainstream programming languages like Rust. It's a common software engineering practice to compose applications from pieces written in multiple languages, using each to their own strength. For example, AI solutions traditionally use the python scripting language for convenient developer experience, while the underlying AI models get implemented in a lower level language such as C++.

The same pattern can of course be applied to dApps, where we'd expect application specific languages like Solidity mixed with libraries implementing computationally complex algorithms in a lower level language. Business logic and user interfaces are naturally implemented as regular Solidity dApps which can include (link against) Rust libraries. Rust is a fast, safe low level language and the Polkadot SDK is written in Rust itself, making it an excellent choice.

For example, [ZK proof verifiers](https://en.wikipedia.org/wiki/Zero-knowledge_proof) or expensive [DeFi](https://en.wikipedia.org/wiki/Decentralized_finance) primitives would benefit greatly from Rust implementations. 

`revive` provides tooling support and a small Rust contracts SDK for seamless integration with Rust libraries.
