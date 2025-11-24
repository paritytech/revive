# JS NPM package

The `resolc` compiler driver is published as NPM package under [@parity/resolc](https://www.npmjs.com/package/@parity/resolc). 

It's usable from `node.js` code or directly from the command line:

```
npx @parity/resolc@latest --bin crates/integration/contracts/flipper.sol -o /tmp/out
```

> **Note**
>
> While the npm package makes a nice portable option, it doesn't exposes all options.
