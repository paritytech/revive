# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [Baseline - To LLVM IR](#baseline---to-llvm-ir)
    - [ERC20 - To LLVM IR](#erc20---to-llvm-ir)
    - [SHA1 - To LLVM IR](#sha1---to-llvm-ir)
    - [Storage - To LLVM IR](#storage---to-llvm-ir)
    - [Transfer - To LLVM IR](#transfer---to-llvm-ir)
    - [Baseline - Parse](#baseline---parse)
    - [ERC20 - Parse](#erc20---parse)
    - [SHA1 - Parse](#sha1---parse)
    - [Storage - Parse](#storage---parse)
    - [Transfer - Parse](#transfer---parse)

## Benchmark Results

### Baseline - To LLVM IR

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `110.12 us` (✅ **1.00x**)  |

### ERC20 - To LLVM IR

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `596.31 us` (✅ **1.00x**)  |

### SHA1 - To LLVM IR

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `328.11 us` (✅ **1.00x**)  |

### Storage - To LLVM IR

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `148.46 us` (✅ **1.00x**)  |

### Transfer - To LLVM IR

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `158.24 us` (✅ **1.00x**)  |

### Baseline - Parse

|        | `Revive`                 |
|:-------|:------------------------ |
|        | `8.10 us` (✅ **1.00x**)  |

### ERC20 - Parse

|        | `Revive`                   |
|:-------|:-------------------------- |
|        | `151.94 us` (✅ **1.00x**)  |

### SHA1 - Parse

|        | `Revive`                  |
|:-------|:------------------------- |
|        | `70.98 us` (✅ **1.00x**)  |

### Storage - Parse

|        | `Revive`                  |
|:-------|:------------------------- |
|        | `15.24 us` (✅ **1.00x**)  |

### Transfer - Parse

|        | `Revive`                  |
|:-------|:------------------------- |
|        | `18.01 us` (✅ **1.00x**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

