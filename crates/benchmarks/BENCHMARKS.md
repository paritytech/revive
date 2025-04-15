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

|         | `EVM`                    | `PVMInterpreter`                 |
|:--------|:-------------------------|:-------------------------------- |
| **`0`** | `10.08 us` (✅ **1.00x**) | `10.32 us` (✅ **1.02x slower**)  |

### OddPorduct

|              | `EVM`                     | `PVMInterpreter`                 |
|:-------------|:--------------------------|:-------------------------------- |
| **`10000`**  | `3.60 ms` (✅ **1.00x**)   | `1.57 ms` (🚀 **2.28x faster**)   |
| **`100000`** | `34.72 ms` (✅ **1.00x**)  | `14.82 ms` (🚀 **2.34x faster**)  |
| **`300000`** | `105.01 ms` (✅ **1.00x**) | `44.11 ms` (🚀 **2.38x faster**)  |

### TriangleNumber

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `2.43 ms` (✅ **1.00x**)  | `1.12 ms` (🚀 **2.17x faster**)   |
| **`100000`** | `24.20 ms` (✅ **1.00x**) | `10.86 ms` (🚀 **2.23x faster**)  |
| **`360000`** | `88.69 ms` (✅ **1.00x**) | `38.46 ms` (🚀 **2.31x faster**)  |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `144.17 us` (✅ **1.00x**) | `150.85 us` (✅ **1.05x slower**)  |
| **`16`** | `938.71 us` (✅ **1.00x**) | `922.11 us` (✅ **1.02x faster**)  |
| **`20`** | `6.54 ms` (✅ **1.00x**)   | `6.20 ms` (✅ **1.05x faster**)    |
| **`24`** | `45.73 ms` (✅ **1.00x**)  | `41.98 ms` (✅ **1.09x faster**)   |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `23.00 us` (✅ **1.00x**) | `31.88 us` (❌ *1.39x slower*)    |
| **`128`** | `35.28 us` (✅ **1.00x**) | `42.43 us` (❌ *1.20x slower*)    |
| **`256`** | `60.12 us` (✅ **1.00x**) | `61.20 us` (✅ **1.02x slower**)  |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `23.01 us` (✅ **1.00x**) | `47.74 us` (❌ *2.07x slower*)    |
| **`128`** | `25.44 us` (✅ **1.00x**) | `49.67 us` (❌ *1.95x slower*)    |
| **`256`** | `28.66 us` (✅ **1.00x**) | `53.01 us` (❌ *1.85x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `135.87 us` (✅ **1.00x**) | `243.75 us` (❌ *1.79x slower*)    |
| **`64`**  | `258.45 us` (✅ **1.00x**) | `355.70 us` (❌ *1.38x slower*)    |
| **`512`** | `1.10 ms` (✅ **1.00x**)   | `1.09 ms` (✅ **1.01x faster**)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

