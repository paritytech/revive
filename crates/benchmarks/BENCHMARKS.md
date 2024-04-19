# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Baseline](#baseline)
    - [OddProduct](#oddproduct)
    - [TriangleNumber](#trianglenumber)
    - [FibonacciRecursive](#fibonaccirecursive)
    - [FibonacciIterative](#fibonacciiterative)
    - [FibonacciIterativeUnchecked](#fibonacciiterativeunchecked)
    - [FibonacciBinet](#fibonaccibinet)
    - [FibonacciPrepare](#fibonacciprepare)

## Benchmark Results

### Baseline

|        | `EVM`                   | `PolkaVMInterpreter`           | `PolkaVM`                           |
|:-------|:------------------------|:-------------------------------|:----------------------------------- |
|        | `1.04 us` (✅ **1.00x**) | `1.24 us` (❌ *1.19x slower*)   | `107.78 us` (❌ *103.20x slower*)    |

### OddProduct

|                 | `EVM`                     | `PolkaVMInterpreter`           | `PolkaVM`                          |
|:----------------|:--------------------------|:-------------------------------|:---------------------------------- |
| **`2000000`**   | `976.65 ms` (✅ **1.00x**) | `1.03 s` (✅ **1.05x slower**)  | `17.16 ms` (🚀 **56.93x faster**)   |
| **`4000000`**   | `1.97 s` (✅ **1.00x**)    | `2.05 s` (✅ **1.04x slower**)  | `34.98 ms` (🚀 **56.20x faster**)   |
| **`8000000`**   | `3.99 s` (✅ **1.00x**)    | `3.92 s` (✅ **1.02x faster**)  | `65.54 ms` (🚀 **60.81x faster**)   |
| **`120000000`** | `56.57 s` (✅ **1.00x**)   | `59.82 s` (✅ **1.06x slower**) | `992.12 ms` (🚀 **57.02x faster**)  |

### TriangleNumber

|                 | `EVM`                   | `PolkaVMInterpreter`           | `PolkaVM`                         |
|:----------------|:------------------------|:-------------------------------|:--------------------------------- |
| **`3000000`**   | `1.07 s` (✅ **1.00x**)  | `1.27 s` (❌ *1.19x slower*)    | `21.33 ms` (🚀 **50.17x faster**)  |
| **`6000000`**   | `2.15 s` (✅ **1.00x**)  | `2.56 s` (❌ *1.19x slower*)    | `42.63 ms` (🚀 **50.37x faster**)  |
| **`12000000`**  | `4.37 s` (✅ **1.00x**)  | `5.23 s` (❌ *1.20x slower*)    | `85.61 ms` (🚀 **51.01x faster**)  |
| **`180000000`** | `62.02 s` (✅ **1.00x**) | `73.70 s` (❌ *1.19x slower*)   | `1.23 s` (🚀 **50.32x faster**)    |

### FibonacciRecursive

|          | `EVM`                     | `PolkaVMInterpreter`             | `PolkaVM`                          |
|:---------|:--------------------------|:---------------------------------|:---------------------------------- |
| **`8`**  | `42.38 us` (✅ **1.00x**)  | `81.59 us` (❌ *1.93x slower*)    | `109.19 us` (❌ *2.58x slower*)     |
| **`12`** | `263.24 us` (✅ **1.00x**) | `549.62 us` (❌ *2.09x slower*)   | `113.09 us` (🚀 **2.33x faster**)   |
| **`16`** | `1.81 ms` (✅ **1.00x**)   | `3.76 ms` (❌ *2.08x slower*)     | `176.71 us` (🚀 **10.22x faster**)  |
| **`18`** | `4.93 ms` (✅ **1.00x**)   | `10.26 ms` (❌ *2.08x slower*)    | `283.79 us` (🚀 **17.37x faster**)  |
| **`20`** | `12.69 ms` (✅ **1.00x**)  | `26.44 ms` (❌ *2.08x slower*)    | `560.94 us` (🚀 **22.62x faster**)  |

### FibonacciIterative

|           | `EVM`                     | `PolkaVMInterpreter`             | `PolkaVM`                         |
|:----------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`32`**  | `16.81 us` (✅ **1.00x**)  | `49.29 us` (❌ *2.93x slower*)    | `102.39 us` (❌ *6.09x slower*)    |
| **`64`**  | `31.15 us` (✅ **1.00x**)  | `100.26 us` (❌ *3.22x slower*)   | `109.85 us` (❌ *3.53x slower*)    |
| **`128`** | `61.80 us` (✅ **1.00x**)  | `201.59 us` (❌ *3.26x slower*)   | `109.20 us` (❌ *1.77x slower*)    |
| **`256`** | `123.83 us` (✅ **1.00x**) | `399.13 us` (❌ *3.22x slower*)   | `113.34 us` (✅ **1.09x faster**)  |

### FibonacciIterativeUnchecked

|              | `EVM`                     | `PolkaVMInterpreter`             | `PolkaVM`                         |
|:-------------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`32`**     | `13.63 us` (✅ **1.00x**)  | `36.15 us` (❌ *2.65x slower*)    | `102.15 us` (❌ *7.50x slower*)    |
| **`64`**     | `24.45 us` (✅ **1.00x**)  | `74.48 us` (❌ *3.05x slower*)    | `103.97 us` (❌ *4.25x slower*)    |
| **`128`**    | `44.89 us` (✅ **1.00x**)  | `146.27 us` (❌ *3.26x slower*)   | `110.78 us` (❌ *2.47x slower*)    |
| **`256`**    | `85.02 us` (✅ **1.00x**)  | `279.19 us` (❌ *3.28x slower*)   | `106.89 us` (❌ *1.26x slower*)    |
| **`4096`**   | `1.34 ms` (✅ **1.00x**)   | `4.64 ms` (❌ *3.46x slower*)     | `170.55 us` (🚀 **7.87x faster**)  |
| **`300000`** | `102.65 ms` (✅ **1.00x**) | `341.42 ms` (❌ *3.33x slower*)   | `4.35 ms` (🚀 **23.59x faster**)   |

### FibonacciBinet

|           | `EVM`                    | `PolkaVMInterpreter`             | `PolkaVM`                         |
|:----------|:-------------------------|:---------------------------------|:--------------------------------- |
| **`32`**  | `17.15 us` (✅ **1.00x**) | `109.84 us` (❌ *6.41x slower*)   | `113.53 us` (❌ *6.62x slower*)    |
| **`64`**  | `19.76 us` (✅ **1.00x**) | `130.75 us` (❌ *6.62x slower*)   | `112.89 us` (❌ *5.71x slower*)    |
| **`128`** | `21.67 us` (✅ **1.00x**) | `143.94 us` (❌ *6.64x slower*)   | `112.77 us` (❌ *5.20x slower*)    |
| **`256`** | `24.68 us` (✅ **1.00x**) | `173.08 us` (❌ *7.01x slower*)   | `109.64 us` (❌ *4.44x slower*)    |

### FibonacciPrepare

|         | `EvmBinet`               | `EvmIterative`                  | `PolkaVMBinetInterpreter`          | `PolkaVMBinet`                     | `PolkaVMIterativeInterpreter`          | `PolkaVMIterative`                  |
|:--------|:-------------------------|:--------------------------------|:-----------------------------------|:-----------------------------------|:---------------------------------------|:----------------------------------- |
| **`0`** | `98.39 ns` (✅ **1.00x**) | `97.42 ns` (✅ **1.01x faster**) | `39.02 us` (❌ *396.62x slower*)    | `2.97 ms` (❌ *30175.62x slower*)   | `20.57 us` (❌ *209.08x slower*)        | `2.93 ms` (❌ *29736.24x slower*)    |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

