# Known issues

The following is known and we are either working on it or it is a hard limitation. Please do not open a new issue.

## Release

`0.1.0-dev-2`

## Missing features

- [Libraries with public functions are not supported](https://github.com/paritytech/revive/issues/91)
- [Automatic import resolution is not supported](https://github.com/paritytech/revive/issues/98)
- The emulated EVM linear contract memory is limited to 64kb in size. Will be fixed with support for metered dynamic memory.
- [The contract calldata is currently limited to 1kb in size](https://github.com/paritytech/revive/issues/57)
- [EIP-4844 opcodes are not supported](https://github.com/paritytech/revive/issues/64)
- [Delegate calls are not supported](https://github.com/paritytech/revive/issues/67)
- [The `blockhash` opcode is not supported](https://github.com/paritytech/revive/issues/61)
- [The `extcodesize` opcode is not supported](https://github.com/paritytech/revive/issues/58)
- [The `origin` opcode is not supported](https://github.com/paritytech/revive/issues/59)
- [Gas limits for contract calls are ignored](https://github.com/paritytech/revive/issues/60)
- [Gas related opcodes are not supported](https://github.com/paritytech/revive/issues/60)
- IPFS metadata hashes are not supported
- [Compiled contract artifacts can exceed the pallet static memory limit and fail to deploy](https://github.com/paritytech/revive/issues/96).
- [Transfers to inexistant accounts will fail if the transferred value lies below the ED.](https://github.com/paritytech/revive/issues/83) Will be fixed in the pallet to make the ED completely transparent for contracts.

## Wontfix

Please consult our documentation to learn more about Solidity and EVM features likely to remain unsupported (and why they will not be supported).

TODO: Insert link to the relevant documentation section.
