# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Empty](#empty)
    - [Dependency](#dependency)
    - [LargeDivRem](#largedivrem)
    - [Yul Memset](#yul-memset)
    - [Yul Return](#yul-return)
    - [Std JSON Codegen All Files](#std-json-codegen-all-files)
    - [Std JSON Codegen One of Many Files](#std-json-codegen-one-of-many-files)
    - [Std JSON No Codegen](#std-json-no-codegen)

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

### Std JSON Codegen All Files

|        | `resolc`                | `solc`                          |
|:-------|:------------------------|:------------------------------- |
|        | `14.59 s` (✅ **1.00x**) | `13.86 s` (✅ **1.05x faster**)  |

### Std JSON Codegen One of Many Files

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `456.07 ms` (✅ **1.00x**) | `13.78 s` (❌ *30.22x slower*)    |

### Std JSON No Codegen

|        | `resolc`                  | `solc`                            |
|:-------|:--------------------------|:--------------------------------- |
|        | `365.40 ms` (✅ **1.00x**) | `354.96 ms` (✅ **1.03x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

