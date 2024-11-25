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
| **`0`** | `1.86 us` (✅ **1.00x**) | `9.62 us` (❌ *5.19x slower*)    |

### OddPorduct

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `2.56 ms` (✅ **1.00x**)  | `1.32 ms` (🚀 **1.94x faster**)   |
| **`100000`** | `25.02 ms` (✅ **1.00x**) | `13.06 ms` (🚀 **1.92x faster**)  |
| **`300000`** | `77.20 ms` (✅ **1.00x**) | `39.25 ms` (🚀 **1.97x faster**)  |

### TriangleNumber

|              | `EVM`                    | `PVMInterpreter`                  |
|:-------------|:-------------------------|:--------------------------------- |
| **`10000`**  | `1.90 ms` (✅ **1.00x**)  | `946.82 us` (🚀 **2.01x faster**)  |
| **`100000`** | `18.54 ms` (✅ **1.00x**) | `9.00 ms` (🚀 **2.06x faster**)    |
| **`360000`** | `66.49 ms` (✅ **1.00x**) | `32.63 ms` (🚀 **2.04x faster**)   |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `116.98 us` (✅ **1.00x**) | `98.32 us` (✅ **1.19x faster**)   |
| **`16`** | `788.62 us` (✅ **1.00x**) | `577.08 us` (✅ **1.37x faster**)  |
| **`20`** | `5.39 ms` (✅ **1.00x**)   | `3.83 ms` (✅ **1.41x faster**)    |
| **`24`** | `36.71 ms` (✅ **1.00x**)  | `26.34 ms` (✅ **1.39x faster**)   |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `13.05 us` (✅ **1.00x**) | `22.03 us` (❌ *1.69x slower*)    |
| **`128`** | `23.71 us` (✅ **1.00x**) | `30.90 us` (❌ *1.30x slower*)    |
| **`256`** | `44.76 us` (✅ **1.00x**) | `48.17 us` (✅ **1.08x slower**)  |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `11.26 us` (✅ **1.00x**) | `26.94 us` (❌ *2.39x slower*)    |
| **`128`** | `12.76 us` (✅ **1.00x**) | `28.58 us` (❌ *2.24x slower*)    |
| **`256`** | `14.41 us` (✅ **1.00x**) | `29.65 us` (❌ *2.06x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `85.04 us` (✅ **1.00x**)  | `163.77 us` (❌ *1.93x slower*)    |
| **`64`**  | `163.24 us` (✅ **1.00x**) | `245.35 us` (❌ *1.50x slower*)    |
| **`512`** | `720.15 us` (✅ **1.00x**) | `782.98 us` (✅ **1.09x slower**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

