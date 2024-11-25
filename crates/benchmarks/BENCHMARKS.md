# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Baseline](#baseline)
    - [OddPorduct](#oddporduct)
    - [TriangleNumber](#trianglenumber)
    - [FibonacciRecursive](#fibonaccirecursive)
    - [FibonacciIterative](#fibonacciiterative)
    - [FibonacciBinet](#fibonaccibinet)
    - [SHA1](#sha1)

## Benchmark Results

### Baseline

|         | `EVM`                   | `PVMInterpreter`                |
|:--------|:------------------------|:------------------------------- |
| **`0`** | `1.81 us` (✅ **1.00x**) | `9.93 us` (❌ *5.49x slower*)    |

### OddPorduct

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `2.56 ms` (✅ **1.00x**)  | `1.28 ms` (🚀 **2.00x faster**)   |
| **`100000`** | `25.84 ms` (✅ **1.00x**) | `12.67 ms` (🚀 **2.04x faster**)  |
| **`300000`** | `75.30 ms` (✅ **1.00x**) | `37.53 ms` (🚀 **2.01x faster**)  |

### TriangleNumber

|              | `EVM`                    | `PVMInterpreter`                  |
|:-------------|:-------------------------|:--------------------------------- |
| **`10000`**  | `1.86 ms` (✅ **1.00x**)  | `856.66 us` (🚀 **2.18x faster**)  |
| **`100000`** | `18.47 ms` (✅ **1.00x**) | `8.48 ms` (🚀 **2.18x faster**)    |
| **`360000`** | `66.99 ms` (✅ **1.00x**) | `30.36 ms` (🚀 **2.21x faster**)   |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `117.47 us` (✅ **1.00x**) | `98.81 us` (✅ **1.19x faster**)   |
| **`16`** | `793.81 us` (✅ **1.00x**) | `584.69 us` (✅ **1.36x faster**)  |
| **`20`** | `5.38 ms` (✅ **1.00x**)   | `3.87 ms` (✅ **1.39x faster**)    |
| **`24`** | `36.64 ms` (✅ **1.00x**)  | `26.41 ms` (✅ **1.39x faster**)   |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `13.08 us` (✅ **1.00x**) | `22.18 us` (❌ *1.70x slower*)    |
| **`128`** | `23.90 us` (✅ **1.00x**) | `31.38 us` (❌ *1.31x slower*)    |
| **`256`** | `45.28 us` (✅ **1.00x**) | `48.77 us` (✅ **1.08x slower**)  |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `11.30 us` (✅ **1.00x**) | `27.66 us` (❌ *2.45x slower*)    |
| **`128`** | `12.84 us` (✅ **1.00x**) | `28.65 us` (❌ *2.23x slower*)    |
| **`256`** | `14.48 us` (✅ **1.00x**) | `29.61 us` (❌ *2.05x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `84.18 us` (✅ **1.00x**)  | `162.77 us` (❌ *1.93x slower*)    |
| **`64`**  | `162.26 us` (✅ **1.00x**) | `239.73 us` (❌ *1.48x slower*)    |
| **`512`** | `710.41 us` (✅ **1.00x**) | `777.52 us` (✅ **1.09x slower**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

