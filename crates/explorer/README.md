# revive-runner

The revive runner is a helper utility aiding in contract debugging.

Given a PVM contract blob, it will upload, deploy and call that contract using a local, stand-alone un-blockchained pallet revive (which is our execution layer).

This is somewhat similar to the geth `evm` utility binary.

## Installation

The `revive-runner` does not depend on the compiler itself, hence installing this utility does not depend on LLVM, so no LLVM build is required.

Inside the root `revive` repository directory, execute:

```bash
make install-revive-runner
```

Which will install the `revive-runner` using `cargo`. 

## Usage

Set the `RUST_LOG` environment varibale to the `trace` level to see the full PolkaVM execution trace. For example:

```bash
RUST_LOG=trace revive-runner -f mycontract.pvm -c a9059cbb000000000000000000000000f24ff3a9cf04c71dbc94d0b566f7a27b94566cac0000000000000000000000000000000000000000000000000000000000000000
