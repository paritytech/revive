# Welcome

Hello and a warm welcome to the `revive` Solidity compiler book!

> [!WARNING]
>
> Solidity on PVM is running on the `pallet-revive` runtime. This introduces **observable semantic differences** in comparison with the EVM.
> 
> Study the [differences](https://paritytech.github.io/revive/user_guide/differences.html) section carefully. **Ignoring these differences may lead to defunct contracts.**
> 
> Notable examples:
> - The 63/64 gas rule isn't implemented in the pallet (introduces potential DoS vector when calling other contracts)
> - Contract instantiation works differently (by hash instead of by code)
> - The gas model implemented by `pallet-revive` differs from Ethereum 
> - The heap size is fixed instead of gas-metered and there's a fixed amount of stack size (contracts working fine on EVM may trap on PVM)

## Target audience

- **Solidity dApp developers** should read the [user guide](./user_guide.md). Solidity on PolkaVM introduces important differences to EVM which should be well understood.
- **Contributors** will find the [developer guide](./developer_guide.md) helpful for getting up to speed.

## Other Polkadot contracts resources

Head to [contracts.polkadot.io](https://docs.polkadot.com/develop/smart-contracts/) for more general information about contracts on Polkadot.

## About

This [mdBook](https://github.com/rust-lang/mdBook) documents the revive Solidity compiler project. The content is found under `book/`. Run `make book` to observe changes.
