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
| **`0`** | `5.97 us` (✅ **1.00x**) | `27.04 us` (❌ *4.53x slower*)    |

### OddPorduct

|              | `EVM`                     | `PVMInterpreter`                 |
|:-------------|:--------------------------|:-------------------------------- |
| **`10000`**  | `4.26 ms` (✅ **1.00x**)   | `2.88 ms` (✅ **1.48x faster**)   |
| **`100000`** | `42.37 ms` (✅ **1.00x**)  | `28.35 ms` (✅ **1.49x faster**)  |
| **`300000`** | `127.88 ms` (✅ **1.00x**) | `88.43 ms` (✅ **1.45x faster**)  |

### TriangleNumber

|              | `EVM`                     | `PVMInterpreter`                 |
|:-------------|:--------------------------|:-------------------------------- |
| **`10000`**  | `2.85 ms` (✅ **1.00x**)   | `2.37 ms` (✅ **1.20x faster**)   |
| **`100000`** | `27.85 ms` (✅ **1.00x**)  | `23.01 ms` (✅ **1.21x faster**)  |
| **`360000`** | `103.01 ms` (✅ **1.00x**) | `83.66 ms` (✅ **1.23x faster**)  |

### FibonacciRecursive

|          | `EVM`                     | `PVMInterpreter`                  |
|:---------|:--------------------------|:--------------------------------- |
| **`12`** | `195.19 us` (✅ **1.00x**) | `333.53 us` (❌ *1.71x slower*)    |
| **`16`** | `1.22 ms` (✅ **1.00x**)   | `1.97 ms` (❌ *1.62x slower*)      |
| **`20`** | `8.14 ms` (✅ **1.00x**)   | `13.20 ms` (❌ *1.62x slower*)     |
| **`24`** | `55.09 ms` (✅ **1.00x**)  | `88.56 ms` (❌ *1.61x slower*)     |

### FibonacciIterative

|           | `EVM`                    | `PVMInterpreter`                  |
|:----------|:-------------------------|:--------------------------------- |
| **`64`**  | `33.39 us` (✅ **1.00x**) | `86.02 us` (❌ *2.58x slower*)     |
| **`128`** | `52.91 us` (✅ **1.00x**) | `126.38 us` (❌ *2.39x slower*)    |
| **`256`** | `82.33 us` (✅ **1.00x**) | `208.74 us` (❌ *2.54x slower*)    |

### FibonacciBinet

|           | `EVM`                    | `PVMInterpreter`                  |
|:----------|:-------------------------|:--------------------------------- |
| **`64`**  | `32.29 us` (✅ **1.00x**) | `161.75 us` (❌ *5.01x slower*)    |
| **`128`** | `36.02 us` (✅ **1.00x**) | `172.59 us` (❌ *4.79x slower*)    |
| **`256`** | `41.21 us` (✅ **1.00x**) | `185.30 us` (❌ *4.50x slower*)    |

### SHA1

|           | `EVM`                     | `PVMInterpreter`                  |
|:----------|:--------------------------|:--------------------------------- |
| **`1`**   | `160.17 us` (✅ **1.00x**) | `403.46 us` (❌ *2.52x slower*)    |
| **`64`**  | `286.69 us` (✅ **1.00x**) | `479.79 us` (❌ *1.67x slower*)    |
| **`512`** | `1.18 ms` (✅ **1.00x**)   | `1.37 ms` (❌ *1.16x slower*)      |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

