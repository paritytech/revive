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
| **`0`** | `10.63 us` (✅ **1.00x**) | `10.35 us` (✅ **1.03x faster**)  |

### OddPorduct

|              | `EVM`                     | `PVMInterpreter`                 |
|:-------------|:--------------------------|:-------------------------------- |
| **`10000`**  | `3.63 ms` (✅ **1.00x**)   | `1.66 ms` (🚀 **2.19x faster**)   |
| **`100000`** | `36.66 ms` (✅ **1.00x**)  | `16.39 ms` (🚀 **2.24x faster**)  |
| **`300000`** | `108.64 ms` (✅ **1.00x**) | `50.48 ms` (🚀 **2.15x faster**)  |

### TriangleNumber

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `2.59 ms` (✅ **1.00x**)  | `1.20 ms` (🚀 **2.17x faster**)   |
| **`100000`** | `25.50 ms` (✅ **1.00x**) | `11.82 ms` (🚀 **2.16x faster**)  |
| **`360000`** | `91.57 ms` (✅ **1.00x**) | `42.11 ms` (🚀 **2.17x faster**)  |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `149.13 us` (✅ **1.00x**) | `154.35 us` (✅ **1.04x slower**)  |
| **`16`** | `972.01 us` (✅ **1.00x**) | `924.33 us` (✅ **1.05x faster**)  |
| **`20`** | `6.62 ms` (✅ **1.00x**)   | `6.23 ms` (✅ **1.06x faster**)    |
| **`24`** | `45.25 ms` (✅ **1.00x**)  | `43.44 ms` (✅ **1.04x faster**)   |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `22.71 us` (✅ **1.00x**) | `31.48 us` (❌ *1.39x slower*)    |
| **`128`** | `35.32 us` (✅ **1.00x**) | `41.87 us` (❌ *1.19x slower*)    |
| **`256`** | `59.58 us` (✅ **1.00x**) | `63.43 us` (✅ **1.06x slower**)  |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `23.18 us` (✅ **1.00x**) | `47.33 us` (❌ *2.04x slower*)    |
| **`128`** | `24.97 us` (✅ **1.00x**) | `50.37 us` (❌ *2.02x slower*)    |
| **`256`** | `28.25 us` (✅ **1.00x**) | `53.69 us` (❌ *1.90x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `132.75 us` (✅ **1.00x**) | `232.17 us` (❌ *1.75x slower*)    |
| **`64`**  | `240.82 us` (✅ **1.00x**) | `328.19 us` (❌ *1.36x slower*)    |
| **`512`** | `1.03 ms` (✅ **1.00x**)   | `1.03 ms` (✅ **1.01x faster**)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

