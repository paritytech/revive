# FAQ

## What EVM version do you support?

We neither do or do not support any EVM version. We support Solidity versions, starting from `solc` version 0.8.0 onwards.

## Do you support opcode `XY`?

See above, the same applies.

## In what Solidity version should I write my dApp?

We generally recommend to always use the latest supported version to profit from latest bugfixes, features and performance improvements.

Find out about the latest supported version by running `resolc --supported-solc-versions` or checking [here](https://github.com/paritytech/resolc-bin).

## Tool `XY` says the contract size is larger than 24kb and will fail to deploy?

The 24kb code size restriction only exist for the EVM. Our limit is currently around 1mb and may increase further in the future.

## Is `resolc` a drop-in replacement for `solc`?

No. `resolc` sometimes works very similar to `solc` but it's not considered a drop-in replacement.

