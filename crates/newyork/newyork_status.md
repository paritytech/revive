# NewYork IR Implementation Status

## Overview

The newyork IR is a custom intermediate representation between Yul and LLVM IR. It uses SSA form with structured control flow.

**Current Status: COMPLETE**

The newyork IR passes 100% of integration tests (62 of 62 tests pass).

## How to Enable

Set the environment variable:
```bash
export RESOLC_USE_NEWYORK=1
```

To disable (use regular Yul pipeline):
```bash
unset RESOLC_USE_NEWYORK
```

## What Works

All features are fully implemented:
- All arithmetic operations (add, sub, mul, div, mod, exp, etc.)
- All bitwise operations (and, or, xor, shl, shr, sar, byte, clz)
- All comparison operations (lt, gt, eq, iszero, slt, sgt)
- Memory operations (mload, mstore, mstore8, mcopy, msize)
- Storage operations (sload, sstore, tload, tstore)
- Calldata operations (calldataload, calldatasize, calldatacopy)
- External calls (call, staticcall, delegatecall)
- Contract creation (create, create2)
- Return/revert handling
- Events/logging (log0-log4)
- Keccak256 hashing
- Context getters (caller, address, origin, value, gas, etc.)
- Block context (number, timestamp, coinbase, basefee, etc.)
- Code operations (codesize, codecopy, extcodesize, extcodehash)
- Return data operations (returndatasize, returndatacopy)
- Immutables (loadImmutable, setImmutable)
- Count leading zeros (clz)
- Data operations (datasize, dataoffset)
- Linker symbols (for library linking)
- Recursive functions
- Complex control flow (switch with SSA merging)

## Test Results

When `RESOLC_USE_NEWYORK=1` is set:
- **62 of 62 integration tests pass (100%)**

All tests pass including:
- flipper, Baseline, Computation, Events
- FibonacciIterative, FibonacciRecursive, FibonacciBinet
- ERC20, Storage, Transfer, Send, Value
- Hash operations (SHA1, Keccak256)
- Bitwise operations, Division operations
- Create, Create2, create2_salt
- ExtCodeHash, ExtCodeSize, ExtCode
- Address, Caller, GasLeft, GasLimit, GasPrice
- MLoad, MStore8, MCopy, mcopy_overlap, MSize
- AddModMulMod, SignedDivision
- CLZ, LayoutAt, SbrkBoundsChecks, SafeTruncate
- Block, BlockHash, BaseFee, Coinbase
- Delegate, Immutables
- LinkerSymbol
- And more...

When `RESOLC_USE_NEWYORK` is NOT set (default):
- All 62 integration tests pass (100%)
- Regular Yul pipeline is fully functional

## Architecture

```
Yul Source
    |
[from_yul.rs] Yul AST -> NewYork IR (SSA form)
    |
[to_llvm.rs] NewYork IR -> LLVM IR
    |
LLVM Backend -> PolkaVM bytecode
```

Key files:
- `ir.rs` - IR data structures (Value, Expr, Statement, Function, Object)
- `ssa.rs` - SSA builder for variable tracking
- `from_yul.rs` - Translation from Yul AST to newyork IR
- `to_llvm.rs` - LLVM code generation from newyork IR
- `type_inference.rs` - Type inference for integer narrowing
- `heap_opt.rs` - Heap optimization analysis

## Past Fixes

1. **codesize() in deploy vs runtime** - Fixed to return calldatasize in deploy code, extcodesize(self) in runtime code
2. **Switch statement SSA handling** - Properly merge modified variables across switch branches
3. **Leave statement return values** - Store return values before branching to return block
4. **Block scope variable propagation** - Propagate outer-scope variable modifications from nested blocks
5. **Large literal handling** - Fixed truncation of BigUint to u64, now uses string conversion
6. **Switch terminator handling** - Skip unreachable blocks from phi nodes
7. **Function redeclaration ICE** - Fixed function tracking to use internal names with code_type suffix
8. **LinkerSymbol expression** - Added `Expr::LinkerSymbol` variant for library address references

## Usage Warning

Do NOT enable newyork IR for production builds yet. While all tests pass, the IR should undergo more extensive real-world testing before production use.

## Next Steps (Phase 2+)

The newyork IR is ready for Phase 2 optimizations:
1. Type inference - Narrow I256 to I64/I32/I8 where provable
2. Memory optimizations - Load-after-store elimination, dead store elimination
3. Custom inlining - Better decisions than LLVM's generic heuristics
4. Pattern rewrites - Transform EVM idioms to efficient PVM equivalents

See IR_PLAN.md for detailed optimization plans.
