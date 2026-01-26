# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Empty](#empty)
    - [Dependency](#dependency)
    - [LargeDivRem](#largedivrem)
    - [Yul Memset](#yul-memset)
    - [Yul Return](#yul-return)
    - [Std JSON Codegen](#std-json-codegen)
    - [Std JSON Codegen One of Some Files](#std-json-codegen-one-of-some-files)
    - [Std JSON No Codegen Many Files](#std-json-no-codegen-many-files)

## Benchmark Results

### Empty

|        | `resolc`                 | `solc`                           |
|:-------|:-------------------------|:-------------------------------- |
|        | `59.86 ms` (✅ **1.00x**) | `11.44 ms` (🚀 **5.23x faster**)  |

### Dependency

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `164.51 ms` (✅ **1.00x**) | `68.47 ms` (🚀 **2.40x faster**)  |

### LargeDivRem

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `121.55 ms` (✅ **1.00x**) | `25.67 ms` (🚀 **4.74x faster**)  |

### Yul Memset

|        | `resolc`                 | `solc`                           |
|:-------|:-------------------------|:-------------------------------- |
|        | `63.83 ms` (✅ **1.00x**) | `10.39 ms` (🚀 **6.14x faster**)  |

### Yul Return

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `53.55 ms` (✅ **1.00x**) | `8.95 ms` (🚀 **5.99x faster**)  |

### Std JSON Codegen

|        | `resolc`               | `solc`                         |
|:-------|:-----------------------|:------------------------------ |
|        | `1.51 s` (✅ **1.00x**) | `1.11 s` (✅ **1.37x faster**)  |


### Std JSON Codegen One of Some Files

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `454.24 ms` (✅ **1.00x**) | `13.82 s` (❌ *30.42x slower*)    |

### Std JSON No Codegen Many Files

|        | `resolc`               | `solc`                         |
|:-------|:-----------------------|:------------------------------ |
|        | `1.71 s` (✅ **1.00x**) | `1.63 s` (✅ **1.05x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

