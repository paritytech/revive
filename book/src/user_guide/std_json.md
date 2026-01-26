# Standard JSON interface

The `revive` compiler is mostly compatible with the `solc` standard JSON interface. There are a few differences and additional (PVM related) __input__ configurations:

## The `settings.polkavm` object

Used to configure PVM specific compiler settings.

### `settings.polkavm.debugInformation`

A boolean value allowing to enable debug information. Corresponds to `resolc -g`.

### The `settings.polkavm.memoryConfig` object

Used to apply PVM specific memory configuration settings.

#### `settings.polkavm.memoryConfig.heapSize`

A numerical value allowing to configure the contract heap size. Corresponds to `resolc --heap-size`.

#### `settings.polkavm.memoryConfig.stackSize`

A numerical value allowing to configure the contract stack size. Corresponds to `resolc --stack-size`.

## The `settings.optimizer` object

The `settings.optimizer` object is augmented with support for PVM specific optimization settings.

### `settings.optimizer.mode`

A single char value to configure the LLVM optimizer settings. Corresponds to `resolc -O`.

## `settings.llvmArguments`

Allows to specify arbitrary command line arguments to LLVM initialization. Used mainly for development and debugging purposes.

## The `settings.outputSelection` object

Used to select desired outputs.

### The "all" (`*`) wildcard

Resolc supports the "all" (`*`) wildcard for the file-level (first-level) and contract-level (second-level) keys. A file-level key can be either the wildcard or a specific file name, whereas the contract-level key can only be the wildcard for robustness reasons.

Thus, output can be requested 2 ways:

```json
// All files and all contracts:
{
  "settings": {
    "outputSelection": {
      "*": {
        "*": [/* specific contract-level output fields */],
        "": [/* specific file-level output fields */]
      }
    }
  }
}

// Specific files and all their contracts:
{
  "settings": {
    "outputSelection": {
      "path/to/my/file.sol": {
        "*": [/* specific contract-level output fields */],
        "": [/* specific file-level output fields */]
      },
      // Rest of files...
    }
  }
}
```

### Requesting Code Generation

When requesting code generation, such as `evm.bytecode` or `evm.assembly`, the resolc compilation process additionally needs `ast`, `metadata`, `irOptimized`, and `evm.methodIdentifiers` selectors. These selectors will be automatically added if code generation is needed, but will only be included in the output if explicitly requested.

```json
{
  "settings": {
    "outputSelection": {
      "path/to/my/file1.sol": {
        // Contracts in this file will generate bytecode.
        // Only these fields of the JSON output selection will be in the `contracts` output.
        "*": ["abi", "evm.methodIdentifiers", "metadata", "evm.bytecode"],
        // Only this field of the JSON output selection will be in the `sources` output.
        "": ["ast"]
      },
      "path/to/my/file2.sol": {
        // No contracts in this file will generate bytecode.
        "*": ["abi", "evm.methodIdentifiers", "metadata"],
        // No `ast` will be in the `sources` output (only the automatically added `id`,
        // similar to solc as this is not a configurable output selection).
        "": []
      },
    }
  }
}
```
