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

|         | `EVM`                   | `PVMInterpreter`                 |
|:--------|:------------------------|:-------------------------------- |
| **`0`** | `3.36 us` (✅ **1.00x**) | `11.84 us` (❌ *3.52x slower*)    |

### OddPorduct

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `3.11 ms` (✅ **1.00x**)  | `1.53 ms` (🚀 **2.03x faster**)   |
| **`100000`** | `30.70 ms` (✅ **1.00x**) | `15.54 ms` (🚀 **1.98x faster**)  |
| **`300000`** | `92.68 ms` (✅ **1.00x**) | `45.47 ms` (🚀 **2.04x faster**)  |

### TriangleNumber

|              | `EVM`                    | `PVMInterpreter`                 |
|:-------------|:-------------------------|:-------------------------------- |
| **`10000`**  | `2.29 ms` (✅ **1.00x**)  | `1.09 ms` (🚀 **2.11x faster**)   |
| **`100000`** | `22.84 ms` (✅ **1.00x**) | `10.66 ms` (🚀 **2.14x faster**)  |
| **`360000`** | `82.29 ms` (✅ **1.00x**) | `37.01 ms` (🚀 **2.22x faster**)  |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `135.67 us` (✅ **1.00x**) | `125.02 us` (✅ **1.09x faster**)  |
| **`16`** | `903.75 us` (✅ **1.00x**) | `762.79 us` (✅ **1.18x faster**)  |
| **`20`** | `6.12 ms` (✅ **1.00x**)   | `4.96 ms` (✅ **1.23x faster**)    |
| **`24`** | `42.05 ms` (✅ **1.00x**)  | `33.86 ms` (✅ **1.24x faster**)   |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `15.04 us` (✅ **1.00x**) | `29.45 us` (❌ *1.96x slower*)    |
| **`128`** | `26.36 us` (✅ **1.00x**) | `42.19 us` (❌ *1.60x slower*)    |
| **`256`** | `48.61 us` (✅ **1.00x**) | `65.71 us` (❌ *1.35x slower*)    |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                 |
|:----------|:-------------------------|:-------------------------------- |
| **`64`**  | `15.22 us` (✅ **1.00x**) | `41.46 us` (❌ *2.72x slower*)    |
| **`128`** | `17.05 us` (✅ **1.00x**) | `42.84 us` (❌ *2.51x slower*)    |
| **`256`** | `19.00 us` (✅ **1.00x**) | `44.36 us` (❌ *2.34x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `110.04 us` (✅ **1.00x**) | `216.11 us` (❌ *1.96x slower*)    |
| **`64`**  | `209.04 us` (✅ **1.00x**) | `309.48 us` (❌ *1.48x slower*)    |
| **`512`** | `903.65 us` (✅ **1.00x**) | `980.49 us` (✅ **1.09x slower**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

