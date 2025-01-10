# Changelog

## Unreleased

This is a development pre-release.

### Added
- The `revive-llvm-builder` crate with the `revive-llvm` helper utility for streamlined management of the LLVM framework dependency.

### Changed
- The minimum supported Rust version is `1.81.0`.

## v0.1.0-dev.7

This is a development pre-release.

### Added
- Implement the `GASPRICE` opcode.
- Implement the `BASEFEE` opcode.
- Implement the `GASLIMIT` opcode.

### Changed
- The `GAS` opcode now returns the remaining `ref_time`.
- Contracts can now be supplied call data input of arbitrary size.
- Some syscalls now return the value in a register, slightly improving  emitted contract code.
- Calls forward maximum weight limits instead of 0, anticipating a change in polkadot-sdk where weight limits of 0 no longer interprets as uncapped limit.

### Fixed
- A linker bug which was preventing certain contracts from linking with the PVM linker. 
- JS: Fix encoding conversion from JS string (UTF-16) to UTF-8.
- The git commit hash slug is always displayed in the version string. 

## v0.1.0-dev.6

This is a development pre-release.

# Added
- Implement the `BLOCKHASH` opcode.
- Implement delegate calls.
- Implement the `GASPRICE` opcode. Currently hard-coded to return `1`.
- The ELF shared object contract artifact is dumped into the debug output directory.
- Initial support for emitting debug info (opt in via the `-g` flag)

# Changed
- resolc now emits 64bit PolkaVM blobs, reducing contract code size and execution time.
- The RISC-V bit-manipulation target feature (`zbb`) is enabled.

# Fixed
- Compilation to Wasm (for usage in node and web browsers)


## v0.1.0-dev.5

This is development pre-release.

# Added
- Implement the `CODESIZE` and `EXTCODESIZE` opcodes.

# Changed
- Include the full revive version in the contract metadata.

# Fixed

## v0.1.0-dev-4

This is development pre-release.

# Added
- Support the `ORIGIN` opcode.

# Changed
- Update polkavm to `v0.14.0`.
- Enable the `a`, `fast-unaligned-access` and `xtheadcondmov` LLVM target features, decreasing the code size for some contracts.

# Fixed
