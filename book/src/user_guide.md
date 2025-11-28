# `resolc` user guide

This chapter explains how to use `resolc` in different ways as well as some important aspects of compiling Solidity to PolkaVM with `resolc`.

## `revive` vs. `resolc` nomenclature
`revive` is the name of the overarching "Solidity to PolkaVM" compiler project, which contains multiple components (for example the Yul parser, the code generation library, the `resolc` executable itself and many things more).

`resolc` is the name of the compiler driver executable, which transparently uses many `revive` components to produce compiled contract artifacts.

In other words, `revive` is the full compiler infrastructure (more like `LLVM`) and `resolc` is a user-facing single-entrypoint compiler frontend (more like `clang`).
