# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Baseline](#baseline)
    - [OddPorduct](#oddporduct)
    - [TriangleNumber](#trianglenumber)
    - [FibonacciRecursive](#fibonaccirecursive)
    - [FibonacciIterative](#fibonacciiterative)
    - [PrepareBaseline](#preparebaseline)
    - [PrepareOddProduct](#prepareoddproduct)
    - [PrepareTriangleNumber](#preparetrianglenumber)
    - [PrepareFibonacciRecursive](#preparefibonaccirecursive)
    - [PrepareFibonacciIterative](#preparefibonacciiterative)

## Benchmark Results

### Baseline

|         | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:--------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`0`** | `900.78 ns` (✅ **1.00x**) | `715.00 ns` (✅ **1.26x faster**) | `26.22 us` (❌ *29.11x slower*)    |

### OddPorduct

|                 | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:----------------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`300000`**    | `223.94 ms` (✅ **1.00x**) | `125.43 ms` (✅ **1.79x faster**) | `2.56 ms` (🚀 **87.60x faster**)   |
| **`1200000`**   | `907.18 ms` (✅ **1.00x**) | `486.46 ms` (🚀 **1.86x faster**) | `9.96 ms` (🚀 **91.10x faster**)   |
| **`12000000`**  | `9.41 s` (✅ **1.00x**)    | `4.96 s` (🚀 **1.90x faster**)    | `98.26 ms` (🚀 **95.75x faster**)  |
| **`180000000`** | `133.65 s` (✅ **1.00x**)  | `73.98 s` (🚀 **1.81x faster**)   | `1.48 s` (🚀 **90.04x faster**)    |
| **`720000000`** | `543.61 s` (✅ **1.00x**)  | `295.27 s` (🚀 **1.84x faster**)  | `6.14 s` (🚀 **88.55x faster**)    |

### TriangleNumber

|                 | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:----------------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`360000`**    | `174.29 ms` (✅ **1.00x**) | `134.93 ms` (✅ **1.29x faster**) | `2.58 ms` (🚀 **67.43x faster**)   |
| **`1440000`**   | `723.79 ms` (✅ **1.00x**) | `518.44 ms` (✅ **1.40x faster**) | `9.93 ms` (🚀 **72.92x faster**)   |
| **`14400000`**  | `7.03 s` (✅ **1.00x**)    | `5.40 s` (✅ **1.30x faster**)    | `99.93 ms` (🚀 **70.40x faster**)  |
| **`216000000`** | `108.98 s` (✅ **1.00x**)  | `77.85 s` (✅ **1.40x faster**)   | `1.44 s` (🚀 **75.78x faster**)    |
| **`864000000`** | `423.03 s` (✅ **1.00x**)  | `323.22 s` (✅ **1.31x faster**)  | `5.99 s` (🚀 **70.61x faster**)    |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                 | `PVM`                              |
|:---------|:--------------------------|:---------------------------------|:---------------------------------- |
| **`24`** | `80.63 ms` (✅ **1.00x**)  | `159.96 ms` (❌ *1.98x slower*)   | `2.73 ms` (🚀 **29.52x faster**)    |
| **`27`** | `331.93 ms` (✅ **1.00x**) | `662.76 ms` (❌ *2.00x slower*)   | `10.78 ms` (🚀 **30.79x faster**)   |
| **`31`** | `2.35 s` (✅ **1.00x**)    | `4.44 s` (❌ *1.88x slower*)      | `76.69 ms` (🚀 **30.69x faster**)   |
| **`36`** | `26.17 s` (✅ **1.00x**)   | `51.08 s` (❌ *1.95x slower*)     | `819.30 ms` (🚀 **31.94x faster**)  |
| **`39`** | `110.50 s` (✅ **1.00x**)  | `220.00 s` (❌ *1.99x slower*)    | `3.46 s` (🚀 **31.90x faster**)     |

### FibonacciIterative

|                 | `EVM`                     | `PVMInterpreter`                 | `PVM`                              |
|:----------------|:--------------------------|:---------------------------------|:---------------------------------- |
| **`256`**       | `84.27 us` (✅ **1.00x**)  | `291.83 us` (❌ *3.46x slower*)   | `42.87 us` (🚀 **1.97x faster**)    |
| **`162500`**    | `53.32 ms` (✅ **1.00x**)  | `174.85 ms` (❌ *3.28x slower*)   | `2.57 ms` (🚀 **20.78x faster**)    |
| **`650000`**    | `217.77 ms` (✅ **1.00x**) | `699.77 ms` (❌ *3.21x slower*)   | `9.91 ms` (🚀 **21.96x faster**)    |
| **`6500000`**   | `2.14 s` (✅ **1.00x**)    | `6.89 s` (❌ *3.22x slower*)      | `100.67 ms` (🚀 **21.22x faster**)  |
| **`100000000`** | `31.96 s` (✅ **1.00x**)   | `106.46 s` (❌ *3.33x slower*)    | `1.50 s` (🚀 **21.28x faster**)     |
| **`400000000`** | `128.68 s` (✅ **1.00x**)  | `447.34 s` (❌ *3.48x slower*)    | `6.19 s` (🚀 **20.77x faster**)     |

### PrepareBaseline

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `179.68 ns` (✅ **1.00x**) | `506.01 ns` (❌ *2.82x slower*)   | `1.70 us` (❌ *9.45x slower*)         | `29.44 us` (❌ *163.87x slower*)   | `69.01 us` (❌ *384.08x slower*)    |

### PrepareOddProduct

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                     | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:---------------------------------|:---------------------------------- |
| **`0`** | `509.96 ns` (✅ **1.00x**) | `485.20 ns` (✅ **1.05x faster**) | `1.69 us` (❌ *3.32x slower*)         | `29.88 us` (❌ *58.59x slower*)   | `70.20 us` (❌ *137.66x slower*)    |

### PrepareTriangleNumber

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                     | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:---------------------------------|:---------------------------------- |
| **`0`** | `508.44 ns` (✅ **1.00x**) | `528.74 ns` (✅ **1.04x slower**) | `1.83 us` (❌ *3.60x slower*)         | `50.81 us` (❌ *99.94x slower*)   | `68.37 us` (❌ *134.48x slower*)    |

### PrepareFibonacciRecursive

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `409.24 ns` (✅ **1.00x**) | `507.67 ns` (❌ *1.24x slower*)   | `1.80 us` (❌ *4.40x slower*)         | `46.24 us` (❌ *112.99x slower*)   | `69.06 us` (❌ *168.76x slower*)    |

### PrepareFibonacciIterative

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `304.00 ns` (✅ **1.00x**) | `524.75 ns` (❌ *1.73x slower*)   | `1.88 us` (❌ *6.17x slower*)         | `43.50 us` (❌ *143.11x slower*)   | `66.82 us` (❌ *219.80x slower*)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

