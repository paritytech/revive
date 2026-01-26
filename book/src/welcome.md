# Welcome

Hello and a warm welcome to the `revive` Solidity compiler book!

> [!WARNING]
>
> Solidity on PVM is running on the `pallet-revive` runtime. This introduces **observable semantic differences** in comparison with the EVM.
> 
>  Study the [differences](./user_guide/differences.md) section carefully. **Ignoring these differences may lead to serious security bugs in your contract code.**
> 
> Notable examples: The 63/64 gas rule isn't implemented in the pallet (introduces DoS vectors), contract instantiation works differently and the gas model is different.

## Target audience

- **Solidity dApp developers** should read the [user guide](./user_guide.md). Solidity on PolkaVM introduces important differences to EVM which should be well understood.
- **Contributors** will find the [developer guide](./developer_guide.md) helpful for getting up to speed.

## Other Polkadot contracts resources

Head to [contracts.polkadot.io](https://docs.polkadot.com/develop/smart-contracts/) for more general information about contracts on Polkadot.

## About

This [mdBook](https://github.com/rust-lang/mdBook) documents the revive Solidity compiler project. The content is found under `book/`. Run `make book` to observe changes.
