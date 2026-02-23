# Optimization Findings - Agent One

## Summary

Analyzed the newyork optimizer pipeline and OpenZeppelin contracts. Found multiple significant optimization opportunities with concrete evidence.

## Key Findings with Evidence

### 1. Missing Full Simplification Pass After MemOpt/FMP/Keccak

**Evidence:** ERC20 contract shows 807 algebraic identity patterns remaining in optimized LLVM:
```
$ grep -E "(add.*0|mul.*1|sub.*0|and.*-1|or.*0)" /tmp/dbg/erc20.sol.MyToken.optimized.ll | wc -l
807
```

**Location:** `lib.rs:170-187` - After mem_opt, FMP propagation, and keccak folding create new constant expressions, only DCE runs - not full constant folding/copy propagation.

**Impact:** HIGH - This is the single biggest missed opportunity per REVIEW.md. Constants created by FMP (replacing `mload(0x40)` with known value) never propagate through downstream arithmetic.

---

### 2. Excessive ZExt/Trunc Operations (Narrowing Not Working)

**Evidence:** 44 zext operations in ERC20 optimized LLVM:
```
$ grep -c "zext" /tmp/dbg/erc20.sol.MyToken.optimized.ll
44
```

Sample patterns show unnecessary widening:
```
%trunc_0 = trunc i256 %1 to i64
%swap_0 = tail call i64 @llvm.bswap.i64(i64 %trunc_0)
%ext_0 = zext i64 %swapped_3 to i256
```

**Location:** `type_inference.rs:98`, `to_llvm.rs:338,346` - Backward demand analysis collects `max_width` but code generation only reads `min_width`.

**Impact:** HIGH - Values used as memory offsets (needs I64) but defined from I256 operations will never be narrowed at definition site.

---

### 3. Inlining Works Well But Could Be Better

**Evidence:** Functions reduced from 226 to 169 in ERC20:
```
$ grep -c "^define " /tmp/dbg/erc20.sol.MyToken.unoptimized.ll
226
$ grep -c "^define " /tmp/dbg/erc20.sol.MyToken.optimized.ll
169
```

**Location:** `inline.rs` - Inlining policy thresholds at lines 30-49.

**Impact:** MEDIUM - 57 functions inlined (25% reduction). Could inline more small functions.

---

### 4. Function Deduplication Not Aggressive Enough

**Evidence:** Multiple similar abi_encode functions remain separate in LLVM:
- `abi_encode_address_address_runtime`
- `abi_encode_address_uint256_uint256_runtime`
- `abi_encode_uint256_uint256_runtime`

**Location:** `simplify.rs` - Fuzzy dedup doesn't cover statement types with literals (MStore, Return, Revert, Log).

**Impact:** MEDIUM - Functions identical except for literal arguments won't be merged.

---

### 5. Memory Optimization Clears State at Control Flow

**Evidence:** Based on mem_opt.rs:591-626 - ALL tracked state cleared when entering If/Switch blocks.

**Location:** `mem_opt.rs` - Load-after-store elimination doesn't work across control flow.

**Impact:** HIGH - For typical Solidity with lots of if/else (require checks), this kills most mem_opt opportunities.

---

### 6. Opcode Collision in Deduplication (Correctness Bug)

**Evidence:** Both `Keccak256Pair` and `BlobHash` encode as `0x24`:
- `simplify.rs:2706`: `Expr::Keccak256Pair { word0, word1 } => { buf.push(0x24); ... }`
- `simplify.rs:2745`: `Expr::BlobHash { index } => { buf.push(0x24); ... }`

**Location:** `simplify.rs` - Two structurally different functions can hash identically.

**Impact:** CORRECTNESS - Silent mismerging when BlobHash is used.

---

### 7. Heap Optimization All-or-Nothing Gating

**Evidence:** If ANY memory operation has dynamic offset that escapes, entire native byte-order optimization disabled.

**Location:** `heap_opt.rs:89-93,698`

**Impact:** MEDIUM - Single `return(dynamic_offset, 32)` in large contract forces all memory ops to byte-swapped mode.

---

### 8. Redundant Subobject Heap Analysis

**Evidence:** `lib.rs:117` already recurses into subobjects, then lines 124-126 re-analyze them.

**Location:** `lib.rs:117,124-126`

**Impact:** LOW - Performance issue, may corrupt results.

---

## Size Analysis Summary

| Contract    | Yul Size | Unopt LLVM | Opt LLVM | PVM ASM | Final .o |
|------------|----------|------------|----------|---------|----------|
| ERC20      | 316KB    | 751KB      | 466KB    | 601KB   | 325KB    |
| MyGovernor | 564KB    | 1210KB     | 777KB    | 1050KB  | 502KB    |

**Reduction from Yul to final:** ERC20: 74% smaller, Governor: 67% smaller

---

## Recommended Priority Fixes

1. **Add full simplify pass after mem_opt/FMP/keccak** - Lowest effort, highest impact
2. **Wire backward demand into let-binding codegen** - Use max_width for i64 arithmetic when all uses are memory offsets
3. **Fix opcode collision** - Change BlobHash from 0x24 to 0x26
4. **Preserve memory state across control flow** - Save and merge state at join points
5. **Make heap optimization per-region** - Track which specific offsets escape

---

## Conclusion

The newyork optimizer provides significant size reduction (67-74%) but there are clear opportunities for improvement. The highest-impact fix is adding a full simplification pass after the mem_opt/FMP/keccak chain, as this would propagate newly-created constants through arithmetic operations.