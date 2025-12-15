# Changelog

## Unreleased

This is a development pre-release.

Supported `polkadot-sdk` rev: `unstable2507`

### Added
- The comprehensive revive compiler book documentation page: https://paritytech.github.io/revive/
- Support for solc v0.8.31.
- Support for the `clz` Yul builtin.

### Changed
- Instruct the LLVM backend and linker to `--relax` (may lead to smaller contract code size).
- Standard JSON mode: Don't forward EVM bytecode related output selections to solc.
- The supported `polkadot-sdk` release is `unstable2507`.

### Fixed:
- The missing `STOP` instruction at the end of `code` blocks.
- The missing bounds check in the internal sbrk implementation.

## v0.5.0

This is a development pre-release.

Supported `polkadot-sdk` rev: `2509.0.0`

### Added
- Support for `SELFDESTRUCT`.

### Changed
- Emulated EVM heap memory accesses of zero length are never out of bounds.
- Switched to newer and cheaper storage syscalls (omits reads and writes of `0` values).

### Fixed
- Introduced a workaround avoiding compiler crashes caused by a bug in LLVM affecting `SDIV`.
- An off-by-one bug affecting `SDIV` overflow semantics.

## v0.4.1

This is a development pre-release.

Supported `polkadot-sdk` rev: `2503.0.1`

### Changed
- The `ast` output is no longer pruned in standard JSON mode (required for foundry).
- Support `standard_json.output_selection` to also look at per file settings.

## v0.4.0

This is a development pre-release.

Supported `polkadot-sdk` rev: `2503.0.1`

### Changed
- Remove the broken `--llvm-ir` mode.
- Remove the unused fallback for size optimization setting.
- Unlinked contract binaries are emitted as raw ELF objects.

### Added
- Line debug information per YUL builtin and for `if` statements.
- Column numbers in debug information.
- Support for the YUL optimizer details in the standard json input definition.
- The `revive-explorer` compiler utility.
- `revive-yul`: The AST visitor interface.
- The `--link` deploy time linking mode.

### Fixed
- The debug info source file matches the YUL path in `--debug-output-dir`, allowing tools to display the source line. 
- Incosistent type forwarding in JSON output (empty string vs. null object).
- The solc automatic import resolution.
- Compiler panic on missing libraries definition.

## v0.3.0

This is a development pre-release.

Supported `polkadot-sdk` rev: `2503.0.1`

### Fixed

- llvm-context: Bugfix the SAR YUL builtin translation.
- runtime-api: Add the missing `memset` builtin.
- npm package: Bugfix the exports field defined in the `package.json`.


## v0.2.0

This is a development pre-release.

Supported `polkadot-sdk` rev: `2503.0.1`

### Changed

- Removed the license printer from the `resolc` binary.
- EVM bytecode is no longer requested from solc (except in test utils) leading to less compilation work in the pipeline.

### Fixed

- solc-json-interface: Serializing of any custom key in the JSON input is only skipped if not provided.
- npm package resolution no longer fails with an 'ERR_PACKAGE_PATH_NOT_EXPORTED' error for packages defining exports fields in the `package.json`.

## v0.1.0

This is a development pre-release.

Supported `polkadot-sdk` rev: `2503.0.1`

### Added

- Add the PolkaVM heap size, stack size and debug info CLI compiler options to the standard JSON settings. This makes the standard JSON input succint for reproducible builds.

### Changed

- Supported `polkadot-sdk` version is now `2503.0.1`
- The `emsdk` version is now `4.0.9`

### Fixed

## v0.1.0-dev.16

This is a development pre-release.

Supported `polkadot-sdk` rev:`c29e72a8628835e34deb6aa7db9a78a2e4eabcee`

### Added

- Move the npm package from paritytech/js-revive, into this repo. The package `@parity/resolc` will be deployed to npm for each release.
- Support for solc v0.8.30

### Changed

- By default, heavy size optimizations are applied.

### Fixed

- @parity/resolc: The solc dependency package is constrained to the latest supported version, preventing breaking the package ever time a new solc package was released. 
- The resolc npm package no longer ignores the optimizer settings

## v0.1.0-dev.14

This is a development pre-release.

Supported `polkadot-sdk` rev:`c29e72a8628835e34deb6aa7db9a78a2e4eabcee`

### Added

- The `revive-runner` helper utility binary which helps to run contracts locally without a blockchain node.
- Allow configuration of the EVM heap memory size and stack size via CLI flags and JSON input settings.

### Changed

- The default PVM stack memory size was increased from 16kb to 32kb.

### Fixed

- Constructors avoid storing zero sized immutable data on exit.

## v0.1.0-dev.13

This is a development pre-release.

Supported `polkadot-sdk` rev:`c29e72a8628835e34deb6aa7db9a78a2e4eabcee`

### Added

- Support for solc v0.8.29
- Decouples the solc JSON-input-output type definitions from the Solidity fronted and expose them via a dedicated crate.
- `--supported-solc-versions` for `resolc` binary to return a `semver` range of supported `solc` versions.
- Support for passing LLVM command line options via the prcoess input or providing one or more `--llvm-arg='..'` resolc CLI flag. This allows more fine-grained control over the LLVM backend configuration.

### Changed

- Storage keys and values are big endian. This was a pre-mature optimization because for the contract itself it this is a no-op and thus not observable. However we should consider the storage layout as part of the contract ABI. The endianness of transient storage values are still kept as-is.
- Running `resolc` using webkit is no longer supported.

### Fixed

- A missing byte swap for the create2 salt value.

## v0.1.0-dev.12

This is a development pre-release.

Supported `polkadot-sdk` rev: `21f6f0705e53c15aa2b8a5706b208200447774a9`

### Added

- Per file output selection for `--standard-json` mode.
- The `ir` output selection option for `--standard-json` mode.

### Changed

- Improved code size: Large contracts compile to smaller code blobs when enabling aggressive size optimizations (`-Oz`).

### Fixed

## v0.1.0-dev.11

This is a development pre-release.

Supported `polkadot-sdk` rev: `274a781e8ca1a9432c7ec87593bd93214abbff50`

### Added

### Changed

### Fixed

- A bug causing incorrect loads from the emulated EVM linear memory.
- A missing integer truncate after switching to 64bit.

## v0.1.0-dev.10

This is a development pre-release.

Supported `polkadot-sdk` rev: `274a781e8ca1a9432c7ec87593bd93214abbff50`

### Added

- Support for the `coinbase` opcode.
- The resolc web JS version.

### Changed

- Missing the `--overwrite` flag emits an error instead of a warning.
- The `resolc` executable prints the help by default.
- Removed support for legacy EVM assembly (EVMLA) translation.
- integration: identify cached code blobs on source code to fix potential confusions.
- Setting base, include or allow paths in emscripten is now a hard error.
- Employ a heuristic to detect `address.transfer` and `address.send` calls.
  If detected, the re-entrant call flag is not set and 0 deposit limit is endowed.

### Fixed

- Solidity: Add the solc `--libraries` files to sources.
- A data race in tests.
- Fix `broken pipe` errors.
- llvm-builder: Allow warnings.
- solidity: Fix the custom compiler warning messages.

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
- Some syscalls now return the value in a register, slightly improving emitted contract code.
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
