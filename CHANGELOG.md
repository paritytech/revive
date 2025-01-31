# Changelog

## Unreleased

This is a development pre-release.

### Added
- Support for the `coinbase` opcode.

### Changed 

### Fixed
- Solidity: Add the solc `--libraries` files to sources.

## v0.1.0-dev.9

This is a development pre-release.

### Added

### Changed 
- Syscalls with more than 6 arguments now pack them into registers.

### Fixed
- Remove reloading of the resolc.js file (fix issue with relative path in web worker)

## v0.1.0-dev.8

This is a development pre-release.

### Added
- The `revive-llvm-builder` crate with the `revive-llvm` helper utility for streamlined management of the LLVM framework dependency.
- Initial support for running `resolc` in the browser.

### Changed
- Suported contracts runtime is polkadot-sdk git version `d62a90c8c729acd98c7e9a5cab9803b8b211ffc5`.
- The minimum supported Rust version is `1.81.0`.
- Error out early instead of invoking `solc` with invalid base or include path flags.

### Fixed
- Decouple the LLVM target dependency from the LLVM host dependency.
- Do not error out if no files and no errors were produced. This aligns resolc closer to solc.
- Fixes input normalization in the Wasm version.

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
