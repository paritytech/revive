# `resolc` user guide

The `resolc` compiler can be used via the `resolc` CLI driver binary, the NPM package or supported tooling.

## `revive` vs. `resolc` nomenclature
`revive` is the name of the overarching "Solidity to PolkaVM" compiler project, which contains multiple components (for example the YUL frontend but also the `resolc` executable itself).

`resolc` is the name of the single entry-point frontend binary executable, which transparently uses all revive components to produce compiled contract artifacts.
