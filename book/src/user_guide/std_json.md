# Standard JSON interface

The `revive` compiler is mostly compatible with the `solc` standard JSON interface. There are a few additional (PVM related) __input__ configurations:

## The `settings.polkavm` object

Used to configure PVM specific compiler settings.

### `settings.polkavm.debugInformation`

A boolean value allowing to enable debug information. Corresponds to `resolc -g`.

### The `settings.polkavm.memoryConfig` object

Used to apply PVM specific memory configuration settings.

#### `settings.polkavm.memoryConifg.heapSize`

A numerical value allowing to configure the contract heap size. Corresponds to `resolc --heap-size`.

#### `settings.polkavm.memoryConifg.stackSize`

A numerical value allowing to configure the contract stack size. Corresponds to `resolc --stack-size`.

## The `settings.optimizer` object

The `settings.optimizer` object is augmented with support for PVM specific optimization settings.

### `settings.optimizer.mode`

A single char value to configure the LLVM optimizer settings. Corresponds to `resolc -O`.

## `settings.llvmArguments`

Allows to specify arbitrary command line arguments to LLVM initialization. Used mainly for development and debugging purposes.

