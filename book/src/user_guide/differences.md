# Differences to EVM

This section highlights some potentially observable differences in the [YUL EVM dialect](https://docs.soliditylang.org/en/latest/yul.html#evm-dialect) translation compared to Ethereum Solidity.

Solidity developers deploying dApps to [`pallet-revive`](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/revive) ought to read and understand this section well.

## Deploy code vs. runtime code

Our contract runtime does not differentiate between runtime code and deploy (constructor) code.
Instead, both are emitted into a single PVM contract code blob and live on-chain.
Therefore, in EVM terminology, the deploy code equals the runtime code.

> [!TIP]
>
> In constructor code, the `codesize` instruction will return the call data size instead of the actual code blob size.

## Solidity

We are aware of the following differences in the translation of Solidity code.

### `address.creationCode`

This returns the bytecode keccak256 hash instead.

## YUL functions

The below list contains noteworthy differences in the translation of YUL functions.

> [!NOTE]
>
> Many functions receive memory buffer offset pointer or size arguments. Since the PVM pointer size is 32 bit, supplying memory offset or buffer size values above `2^32-1` will trap the contract immediately.

The `solc` compiler ought to always emit valid memory references, so Solidity dApp authors don't need to worry about this unless they deal with low level `assembly` code.

### `mload`, `mstore`, `msize`, `mcopy` (memory related functions) 

In general, revive preserves the memory layout, meaning low level memory operations are supported. However, a few caveats apply:
- The EVM linear heap memory is emulated using a fixed byte buffer of 128kb. This implies that the maximum memory a contract can use is limited to 128kb (on Ethereum, contract memory is capped by gas and therefore varies).
- Thus, accessing memory offsets larger than the fixed buffer size will trap the contract at runtime with an `OutOfBound` error.
- The compiler might detect and optimize unused memory reads and writes, leading to a different `msize` compared to what the EVM would see. 

### `calldataload`, `calldatacopy`

In the constructor code, the offset is ignored and this always returns `0`.

### `codecopy`

Only supported in constructor code.

### `create`, `create2`

Deployments on revive work different than on EVM. In a nutshell: Instead of supplying the deploy code concatenated with the constructor arguments (the EVM deploy model), the [revive runtime expects two pointers](https://docs.rs/pallet-revive/latest/pallet_revive/trait.SyscallDoc.html#tymethod.instantiate):
1. A buffer containing the code hash to deploy.
2. The constructor arguments buffer.

To make contract instantiation using the `new` keyword in Solidity work seamlessly,
`revive` translates the `dataoffset` and `datasize` instructions so that they assume the contract hash instead of the contract code.
The hash is always of constant size.
Thus, `revive` is able to supply the expected code hash and constructor arguments pointer to the runtime. 

> [!WARNING]
>
> This might fall apart in code creating contracts inside `assembly` blocks. **We strongly discourage using the `create` family opcodes to manually craft deployments in `assembly` blocks!** Usually, the reason for using `assembly` blocks is to save gas, which is likely futile on revive due to the underlying differences in the VM architectures, gas models and transaction costs.

### `dataoffset`

Returns the contract hash.

### `datasize`

Returns the contract hash size (constant value of `32`).

### `prevrandao`, `difficulty`

Translates to a constant value of `2500000000000000`.

### `pc`, `extcodecopy`

Only valid to use in EVM (they also have no use case in PVM) and produce a compile time error.

### `blobhash`, `blobbasefee`

Related to the Ethereum rollup model and produce a compile time error. Polkadot offers a superior rollup model, removing the use case for blob data related opcodes.

## Difference regarding the `solc` `via-ir` mode

There are two different compilation pipelines available in `solc` and [there are small differences between them](https://docs.soliditylang.org/en/latest/ir-breaking-changes.html).

Since `resolc` processes the YUL IR, always assume the `solc` IR based codegen behavior for contracts compiled with the `revive` compiler.

### Example: State variable initialization order in inheritance

With `via-ir`, base constructors run before derived state variables are initialized:

```solidity
contract InnerContract {
    uint public innerConstructedStartTokenId;

    constructor() {
        innerConstructedStartTokenId = _startTokenId();
    }

    function _startTokenId() internal view virtual returns (uint) {
        return 0;
    }
}

contract Test is InnerContract {
    uint public START_TOKEN_ID = 1;

    constructor() InnerContract() {
    }

    function _startTokenId() internal view virtual override returns (uint) {
        return START_TOKEN_ID;
    }
}
```

Here, `innerConstructedStartTokenId` in `Test` returns `0` (with legacy EVM codegen it'd return `1`).

