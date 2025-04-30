# Usage from Node.js

```typescript
import { compile } from "@parity/resolc";
const sources = {
["contracts/1_Storage.sol"]: {
    content: readFileSync("fixtures/storage.sol", "utf8"),
}

const out = await compile(sources);
```

# Usage from shell

```bash
 npx @parity/resolc@latest --bin contracts/1_Storage.sol
```
