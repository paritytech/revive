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
|        | `64.27 ms` (✅ **1.00x**) | `7.21 ms` (🚀 **8.91x faster**)  |

### Dependency

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `143.34 ms` (✅ **1.00x**) | `8.18 ms` (🚀 **17.53x faster**)  |

### LargeDivRem

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `110.61 ms` (✅ **1.00x**) | `8.35 ms` (🚀 **13.25x faster**)  |

### Memset (`--yul`)

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `58.10 ms` (✅ **1.00x**) | `8.65 ms` (🚀 **6.72x faster**)  |

### Return (`--yul`)

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `52.73 ms` (✅ **1.00x**) | `8.60 ms` (🚀 **6.13x faster**)  |

### Multiple Contracts (`--standard-json`)

|        | `resolc`               | `solc`                            |
|:-------|:-----------------------|:--------------------------------- |
|        | `1.48 s` (✅ **1.00x**) | `704.03 ms` (🚀 **2.10x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

