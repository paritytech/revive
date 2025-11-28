# Testing strategy


Contributors are encouraged to implement some appropriate unit and integration tests together with any bug fixes or new feature implementations. However, when it comes to testing the code generation logic, our testing strategy goes beyond simple unit and integration tests. This chapter explains how the `revive` compiler implementation is tested for correctness and how we define correctness.

## Bug compatibility with Ethereum Solidity

As a Solidity compiler, we aim to preserve contract code semantics as close as possible to Solidity compiled to EVM with the `solc` reference implementation. As highlighted in the user guide, due to the underlying target difference, this isn't always possible. However, wherever it is possible, we follow the philosophy of [**bug compatibility**](https://en.wikipedia.org/wiki/Bug_compatibility) with the Ethereum contracts stack.

## Differential integration tests

A high level of bug compatibility with Ethereum is ensured through [**differential testing**](https://en.wikipedia.org/wiki/Differential_testing) with the Ethereum `solc` and EVM contracts stack. The [revive-integration](https://crates.io/crates/revive-integration) library is the central integration test utility, providing a set of Solidity integration test cases. Further, it implements differential tests against the reference implementation by combining the [revive-runner](https://crates.io/crates/revive-runner) sandbox, the [go-ethereum EVM tool](https://github.com/ethereum/go-ethereum/tree/master/cmd/evm) and the [revive-differential](https://crates.io/crates/revive-differential).
  
The `revive-runner` library provides a [**declarative**](https://en.wikipedia.org/wiki/Declarative_programming) test [specification format](https://github.com/paritytech/revive/blob/main/crates/runner/src/specs.rs). This vastly simplifies writing differential test cases and removes a lot of room for errors in test logic. Example:

```json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Bitwise"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "3fa4f245"
            }
        }
    ]
}
```

Above example instantiates the `Bitwise` contract and calls it with some defined calldata. The `revive-runner` library implements a helper wrapper to execute test specs on the go-ethereum standalone `evm` tool. This allows the `revive-runner` to execute specs against the EVM and the `pallet-revive` runtime. Key to differential teststing is setting `"differential": true`, resulting in the following:

1. The `Bitwise` contract is compiled to EVM and PVM code.
2. The runner executes the defined `actions` on the EVM and collects all state changes (storage, balance) and execution results.
3. The runner executes each action on the PVM. Observed state changes _after each step_ as well as the final execution result is asserted to  match the EVM counterparts __exactly__.

__Note how we never defined any expected outcome manually.__ Instead, we simply observe and collect the data defining the "correct" outcome.

Differential testing in combination with declerative test specifications proved to be simple yet very effective in ensuring expected Ethereum Solidity semantics on `pallet-revive`.

## The differential testing utility

A lot of nuanced bugs caused by tiny implementation details inside the `revive` compiler _and_ the `pallet-revive` runtime could be identified and eliminated early on thanks to the differential testing strategy. Thus we decided to take this approach further and created a comprehensive test runner and a large suite of more complex test cases.

The [Revive Differential Tests](https://github.com/paritytech/revive-differential-tests/) follows the exact same strategy but implements a much more powerful test spec format, spec runner and reports. This allows differentially testing of much more complex test cases (for example testing Uniswap pair creations and swaps), executed via transactions sent to actual blockchain nodes.

