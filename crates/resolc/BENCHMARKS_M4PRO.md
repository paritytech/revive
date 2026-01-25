# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Empty](#empty)
    - [Dependency](#dependency)
    - [LargeDivRem](#largedivrem)
    - [Yul Memset](#yul-memset)
    - [Yul Return](#yul-return)
    - [Std JSON Codegen](#std-json-codegen)
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
|        | `48.76 ms` (✅ **1.00x**) | `9.06 ms` (🚀 **5.38x faster**)  |

### Std JSON Codegen

|        | `resolc`               | `solc`                         |
|:-------|:-----------------------|:------------------------------ |
|        | `1.51 s` (✅ **1.00x**) | `1.11 s` (✅ **1.37x faster**)  |

### Std JSON No Codegen Many Files

|        | `resolc`               | `solc`                         |
|:-------|:-----------------------|:------------------------------ |
|        | `1.71 s` (✅ **1.00x**) | `1.63 s` (✅ **1.05x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

