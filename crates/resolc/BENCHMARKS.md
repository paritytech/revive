# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Empty](#empty)
    - [Dependency](#dependency)
    - [LargeDivRem](#largedivrem)

## Benchmark Results

### Empty

|        | `resolc`                 | `solc`                          |
|:-------|:-------------------------|:------------------------------- |
|        | `62.70 ms` (✅ **1.00x**) | `8.47 ms` (🚀 **7.40x faster**)  |

### Dependency

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `141.28 ms` (✅ **1.00x**) | `7.93 ms` (🚀 **17.81x faster**)  |

### LargeDivRem

|        | `resolc`                  | `solc`                           |
|:-------|:--------------------------|:-------------------------------- |
|        | `108.51 ms` (✅ **1.00x**) | `8.60 ms` (🚀 **12.61x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

