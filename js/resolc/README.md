# @parity/revive

Node.js module to compile Solidity contracts to Polkavm bytecode, using [Revive](https://github.com/paritytech/revive)

# Usage from Node.js

```typescript
const sources = {
["contracts/1_Storage.sol"]: {
    content: readFileSync("fixtures/storage.sol", "utf8"),
}

const out = await compile(sources);
```

# Usage from shell

```bash
 npx @parity/revive@latest --bin contracts/1_Storage.sol
```
