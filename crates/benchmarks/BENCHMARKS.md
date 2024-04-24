# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Baseline](#baseline)
    - [OddProduct](#oddproduct)
    - [TriangleNumber](#trianglenumber)
    - [FibonacciRecursive](#fibonaccirecursive)
    - [FibonacciIterative](#fibonacciiterative)
    - [FibonacciBinet](#fibonaccibinet)
    - [SHA1](#sha1)
    - [PrepareBaseline](#preparebaseline)
    - [PrepareOddProduct](#prepareoddproduct)
    - [PrepareTriangleNumber](#preparetrianglenumber)
    - [PrepareFibonacciRecursive](#preparefibonaccirecursive)
    - [PrepareFibonacciIterative](#preparefibonacciiterative)
    - [PrepareFibonacciBinet](#preparefibonaccibinet)
    - [PrepareSHA1](#preparesha1)

## Benchmark Results

### Baseline

|         | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:--------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`0`** | `855.27 ns` (✅ **1.00x**) | `729.35 ns` (✅ **1.17x faster**) | `23.19 us` (❌ *27.12x slower*)    |

### OddProduct

|                 | `EVM`                   | `PVMInterpreter`               | `PVM`                             |
|:----------------|:------------------------|:-------------------------------|:--------------------------------- |
| **`2000000`**   | `1.51 s` (✅ **1.00x**)  | `1.11 s` (✅ **1.35x faster**)  | `16.91 ms` (🚀 **89.26x faster**)  |
| **`4000000`**   | `3.12 s` (✅ **1.00x**)  | `2.09 s` (✅ **1.49x faster**)  | `32.48 ms` (🚀 **96.10x faster**)  |
| **`8000000`**   | `6.22 s` (✅ **1.00x**)  | `4.26 s` (✅ **1.46x faster**)  | `65.36 ms` (🚀 **95.23x faster**)  |
| **`120000000`** | `90.60 s` (✅ **1.00x**) | `59.54 s` (✅ **1.52x faster**) | `1.02 s` (🚀 **89.00x faster**)    |

### TriangleNumber

|                 | `EVM`                   | `PVMInterpreter`               | `PVM`                             |
|:----------------|:------------------------|:-------------------------------|:--------------------------------- |
| **`3000000`**   | `1.45 s` (✅ **1.00x**)  | `1.01 s` (✅ **1.43x faster**)  | `20.83 ms` (🚀 **69.67x faster**)  |
| **`6000000`**   | `2.92 s` (✅ **1.00x**)  | `2.08 s` (✅ **1.41x faster**)  | `41.97 ms` (🚀 **69.61x faster**)  |
| **`12000000`**  | `5.88 s` (✅ **1.00x**)  | `4.05 s` (✅ **1.45x faster**)  | `83.03 ms` (🚀 **70.82x faster**)  |
| **`180000000`** | `89.53 s` (✅ **1.00x**) | `59.08 s` (✅ **1.52x faster**) | `1.24 s` (🚀 **72.49x faster**)    |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                 | `PVM`                              |
|:---------|:--------------------------|:---------------------------------|:---------------------------------- |
| **`26`** | `200.07 ms` (✅ **1.00x**) | `478.04 ms` (❌ *2.39x slower*)   | `6.93 ms` (🚀 **28.88x faster**)    |
| **`30`** | `1.37 s` (✅ **1.00x**)    | `3.36 s` (❌ *2.45x slower*)      | `45.17 ms` (🚀 **30.30x faster**)   |
| **`34`** | `9.83 s` (✅ **1.00x**)    | `22.55 s` (❌ *2.29x slower*)     | `306.43 ms` (🚀 **32.08x faster**)  |
| **`38`** | `66.98 s` (✅ **1.00x**)   | `150.55 s` (❌ *2.25x slower*)    | `2.22 s` (🚀 **30.21x faster**)     |

### FibonacciIterative

|                 | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:----------------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`256`**       | `88.32 us` (✅ **1.00x**)  | `294.08 us` (❌ *3.33x slower*)   | `42.46 us` (🚀 **2.08x faster**)   |
| **`100000`**    | `32.88 ms` (✅ **1.00x**)  | `121.70 ms` (❌ *3.70x slower*)   | `1.73 ms` (🚀 **18.97x faster**)   |
| **`1000000`**   | `320.59 ms` (✅ **1.00x**) | `1.25 s` (❌ *3.89x slower*)      | `15.60 ms` (🚀 **20.55x faster**)  |
| **`100000000`** | `33.09 s` (✅ **1.00x**)   | `125.08 s` (❌ *3.78x slower*)    | `1.49 s` (🚀 **22.18x faster**)    |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 | `PVM`                            |
|:----------|:-------------------------|:---------------------------------|:-------------------------------- |
| **`64`**  | `20.15 us` (✅ **1.00x**) | `129.45 us` (❌ *6.42x slower*)   | `39.56 us` (❌ *1.96x slower*)    |
| **`128`** | `22.97 us` (✅ **1.00x**) | `150.62 us` (❌ *6.56x slower*)   | `40.13 us` (❌ *1.75x slower*)    |
| **`256`** | `26.20 us` (✅ **1.00x**) | `165.38 us` (❌ *6.31x slower*)   | `39.70 us` (❌ *1.52x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                 | `PVM`                             |
|:----------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`1`**   | `216.56 us` (✅ **1.00x**) | `328.90 us` (❌ *1.52x slower*)   | `43.54 us` (🚀 **4.97x faster**)   |
| **`64`**  | `442.13 us` (✅ **1.00x**) | `553.22 us` (❌ *1.25x slower*)   | `45.73 us` (🚀 **9.67x faster**)   |
| **`512`** | `1.90 ms` (✅ **1.00x**)   | `2.27 ms` (❌ *1.19x slower*)     | `78.40 us` (🚀 **24.21x faster**)  |

### PrepareBaseline

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `177.34 ns` (✅ **1.00x**) | `10.83 us` (❌ *61.07x slower*)   | `1.33 us` (❌ *7.49x slower*)         | `33.43 us` (❌ *188.48x slower*)   | `69.26 us` (❌ *390.56x slower*)    |

### PrepareOddProduct

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                     | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:---------------------------------|:---------------------------------- |
| **`0`** | `486.78 ns` (✅ **1.00x**) | `11.43 us` (❌ *23.49x slower*)   | `1.35 us` (❌ *2.78x slower*)         | `33.95 us` (❌ *69.75x slower*)   | `68.19 us` (❌ *140.07x slower*)    |

### PrepareTriangleNumber

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `489.04 ns` (✅ **1.00x**) | `23.99 us` (❌ *49.06x slower*)   | `1.33 us` (❌ *2.72x slower*)         | `61.40 us` (❌ *125.56x slower*)   | `73.01 us` (❌ *149.29x slower*)    |

### PrepareFibonacciRecursive

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `411.19 ns` (✅ **1.00x**) | `22.32 us` (❌ *54.27x slower*)   | `1.43 us` (❌ *3.49x slower*)         | `54.52 us` (❌ *132.59x slower*)   | `68.99 us` (❌ *167.77x slower*)    |

### PrepareFibonacciIterative

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `313.74 ns` (✅ **1.00x**) | `19.15 us` (❌ *61.04x slower*)   | `1.39 us` (❌ *4.44x slower*)         | `48.30 us` (❌ *153.95x slower*)   | `69.20 us` (❌ *220.57x slower*)    |

### PrepareFibonacciBinet

|         | `Evm`                     | `PVMInterpreterCompile`          | `PVMInterpreterInstantiate`          | `PVMCompile`                      | `PVMInstantiate`                  |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:--------------------------------- |
| **`0`** | `700.40 ns` (✅ **1.00x**) | `41.78 us` (❌ *59.65x slower*)   | `1.40 us` (❌ *2.00x slower*)         | `92.23 us` (❌ *131.69x slower*)   | `68.52 us` (❌ *97.83x slower*)    |

### PrepareSHA1

|         | `Evm`                   | `PVMInterpreterCompile`           | `PVMInterpreterInstantiate`          | `PVMCompile`                       | `PVMInstantiate`                  |
|:--------|:------------------------|:----------------------------------|:-------------------------------------|:-----------------------------------|:--------------------------------- |
| **`0`** | `1.77 us` (✅ **1.00x**) | `124.24 us` (❌ *70.39x slower*)   | `1.33 us` (✅ **1.33x faster**)       | `242.14 us` (❌ *137.19x slower*)   | `69.28 us` (❌ *39.25x slower*)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

