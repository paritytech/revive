# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [PrepareBaseline](#preparebaseline)
    - [PrepareOddProduct](#prepareoddproduct)
    - [PrepareTriangleNumber](#preparetrianglenumber)
    - [PrepareFibonacciRecursive](#preparefibonaccirecursive)
    - [PrepareFibonacciIterative](#preparefibonacciiterative)
    - [PrepareFibonacciBinet](#preparefibonaccibinet)
    - [PrepareSHA1](#preparesha1)

## Benchmark Results

### PrepareBaseline

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                      | `PvmInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `171.33 ns` (✅ **1.00x**) | `11.02 us` (❌ *64.30x slower*)   | `1.30 us` (❌ *7.58x slower*)         | `34.18 us` (❌ *199.48x slower*)   | `70.84 us` (❌ *413.48x slower*)    |

### PrepareOddProduct

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                     | `PvmInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:---------------------------------|:---------------------------------- |
| **`0`** | `525.94 ns` (✅ **1.00x**) | `11.42 us` (❌ *21.71x slower*)   | `1.28 us` (❌ *2.44x slower*)         | `33.18 us` (❌ *63.08x slower*)   | `69.00 us` (❌ *131.20x slower*)    |

### PrepareTriangleNumber

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                      | `PvmInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `494.52 ns` (✅ **1.00x**) | `24.32 us` (❌ *49.18x slower*)   | `1.29 us` (❌ *2.60x slower*)         | `58.01 us` (❌ *117.31x slower*)   | `69.56 us` (❌ *140.67x slower*)    |

### PrepareFibonacciRecursive

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                      | `PvmInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `391.94 ns` (✅ **1.00x**) | `21.53 us` (❌ *54.92x slower*)   | `1.30 us` (❌ *3.33x slower*)         | `53.42 us` (❌ *136.30x slower*)   | `69.45 us` (❌ *177.19x slower*)    |

### PrepareFibonacciIterative

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                      | `PvmInstantiate`                   |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:---------------------------------- |
| **`0`** | `327.61 ns` (✅ **1.00x**) | `21.54 us` (❌ *65.75x slower*)   | `1.31 us` (❌ *3.99x slower*)         | `54.87 us` (❌ *167.48x slower*)   | `70.24 us` (❌ *214.40x slower*)    |

### PrepareFibonacciBinet

|         | `Evm`                     | `PvmInterpreterCompile`          | `PvmInterpreterInstantiate`          | `PvmCompile`                      | `PvmInstantiate`                  |
|:--------|:--------------------------|:---------------------------------|:-------------------------------------|:----------------------------------|:--------------------------------- |
| **`0`** | `741.78 ns` (✅ **1.00x**) | `40.70 us` (❌ *54.87x slower*)   | `1.33 us` (❌ *1.79x slower*)         | `90.40 us` (❌ *121.87x slower*)   | `70.30 us` (❌ *94.77x slower*)    |

### PrepareSHA1

|         | `Evm`                   | `PvmInterpreterCompile`           | `PvmInterpreterInstantiate`          | `PvmCompile`                       | `PvmInstantiate`                  |
|:--------|:------------------------|:----------------------------------|:-------------------------------------|:-----------------------------------|:--------------------------------- |
| **`0`** | `1.73 us` (✅ **1.00x**) | `127.02 us` (❌ *73.61x slower*)   | `1.34 us` (✅ **1.29x faster**)       | `244.88 us` (❌ *141.91x slower*)   | `72.19 us` (❌ *41.84x slower*)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

