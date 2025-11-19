# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Empty](#empty)
    - [Dependency](#dependency)
    - [LargeDivRem](#largedivrem)
    - [Memset (`--yul`)](#memset-(`--yul`))
    - [Return (`--yul`)](#return-(`--yul`))
    - [Multiple Contracts (`--standard-json`)](#multiple-contracts-(`--standard-json`))

## Benchmark Results

### Empty

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `62.77 ms` (✅ **1.00x**) | `9.63 ms` (🚀 **6.52x faster**)  |

### Dependency

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `142.28 ms` (✅ **1.00x**) | `57.57 ms` (🚀 **2.47x faster**)  |

### LargeDivRem

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `110.80 ms` (✅ **1.00x**) | `20.96 ms` (🚀 **5.29x faster**)  |

### Memset (`--yul`)

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `58.39 ms` (✅ **1.00x**) | `8.84 ms` (🚀 **6.61x faster**)  |

### Return (`--yul`)

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `52.83 ms` (✅ **1.00x**) | `8.04 ms` (🚀 **6.57x faster**)  |

### Multiple Contracts (`--standard-json`)

|        | `resolc`               | `solc`                            |
|:-------|:-----------------------|:--------------------------------- |
|        | `1.52 s` (✅ **1.00x**) | `623.91 ms` (🚀 **2.44x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

