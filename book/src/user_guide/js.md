# JS NPM package

The `resolc` compiler driver is published as an NPM package under [@parity/resolc](https://www.npmjs.com/package/@parity/resolc). 

It's usable from `Node.js` code or directly from the command line:

```shell
npx @parity/resolc@latest --bin crates/integration/contracts/flipper.sol -o /tmp/out
```

> [!NOTE]
>
> While the npm package makes a nice portable option, it doesn't expose all options.
