# Wide integer extension: measurements

## Phase 2: PVM blobs

The measurement that matters is the size of the linked PVM blob, over 32 contracts at `-Oz`
-- the workload fixtures in `revive-differential-tests` and a set of OpenZeppelin contracts
-- against the same compiler with `RESOLC_DISABLE_WIDE_INTEGERS` set. The corpus goes from
1,814,276 bytes to 833,204 on the stock Yul pipeline, **-54.08%**, and from 1,104,518 to
540,515 on newyork, **-51.06%**. Adding the 128-bit width takes another 5,061 bytes
(**-0.61%**) and 3,904 (**-0.72%**).

Every configuration comes out of one binary: `RESOLC_DISABLE_WIDE_INTEGERS` and
`RESOLC_DISABLE_UNALIGNED_SCALAR_MEM` in the environment select the arms, and
`--llvm-arg=-riscv-revive-i128` the second width.

Three changes on top of phase 1 account for most of the difference between the object-level
`-30%` and this. The extension implies none of the vector extensions, so no vector type is
legal and the spill and copy paths are the extension's own instructions against fixed-size
stack slots. Half the register file is callee saved, so a value live across a call is saved
once rather than at every call site. And `addmod`, `mulmod`, `exp` and `signextend` reach
their instructions, where before revive still called the `stdlib.ll` routines the
instructions exist to replace.

---

Phase 1 of the vector-extension experiment. The goal was to stop LLVM legalizing EVM words into
`i64` limb chains, and instead select one instruction per wide operation with operands living in
wide registers.

Measured on the 15 benchmark contracts under `benchmarks/single-file` (103 modules), the same
corpus used for the earlier carry-extension analysis, which established where revive's code size
goes.
Sizes are taken per section from the compiled object -- code and constant pool separately, because
the extension moves them in opposite directions. The blob figures are above.

---

## 1. Result

**-689,538 bytes, -25.96%** across the 15 benchmark contracts (103 modules). Fourteen of the fifteen
improve. Code and read-only data move in opposite directions and are reported separately:

| | baseline | XReviveVec | delta |
|---|--:|--:|--:|
| **code** (`.text`) | 2,582,358 | **1,806,612** | **-775,746 (-30.04%)** |
| constant pool (`.rodata`) | 74,023 | 160,231 | +86,208 (+116.46%) |
| **combined** | **2,656,381** | **1,966,843** | **-689,538 (-25.96%)** |

Wide values live in the vector registers -- `i256` is a type `VRM2` holds, so at VLEN=128 a wide
value is an LMUL=2 register pair. All 103 modules compile, with no timeouts.

For scale, the carry extension measured -8.64% on this corpus, and it did not change the argument
ABI at all.

### Per benchmark

| benchmark | modules | baseline | XReviveVec | code only |
|---|--:|--:|--:|--:|
| V5QuoteVerifier | 18 | 575,524 | 442,362 (-23.1%) | -26.7% |
| XENCrypto | 10 | 479,234 | 282,072 (**-41.1%**) | -44.8% |
| FiatTokenV1 | 16 | 373,746 | 308,768 (-17.4%) | -22.7% |
| SnapshotPCCSRouter | 13 | 306,187 | 243,915 (-20.3%) | -25.2% |
| AutomataDcapAttestationFee | 17 | 290,231 | 233,851 (-19.4%) | -23.9% |
| Festival | 14 | 189,194 | 147,862 (-21.8%) | -26.3% |
| P2PMarket | 1 | 136,095 | 87,525 (-35.7%) | -37.8% |
| T3rminalDriver | 3 | 134,985 | 87,061 (-35.5%) | -37.8% |
| TetherToken | 3 | 52,163 | 39,047 (-25.1%) | -28.8% |
| ERC20Factory | 2 | 25,486 | 19,662 (-22.9%) | -26.3% |
| DaimoP256Verifier | 1 | 22,045 | 13,815 (**-37.3%**) | -42.7% |
| ERC20 | 1 | 21,629 | 15,655 (-27.6%) | -30.8% |
| Multicall3 | 1 | 21,367 | 18,457 (-13.6%) | -15.8% |
| WETH9 | 2 | 20,436 | 17,420 (-14.8%) | -18.1% |
| Sha256 | 1 | 8,059 | 9,371 (**+16.3%**) | +10.9% |
| **TOTAL** | **103** | **2,656,381** | **1,966,843** | **-30.04%** |

Sha256 is the one regression and the one contract that is a 32-bit algorithm written in 256-bit
words; see the constant-materialization discussion in §2 and the narrowing work in §7.

### Where the size goes

Instruction mix for XENCrypto, the benchmark with the most wide arithmetic:

| | baseline | XReviveVec |
|---|--:|--:|
| total instructions | 27,926 | **10,418** |
| `sltu` | 704 | **0** |

`sltu` reaching zero is the direct confirmation of the goal: every one of those was materializing a
carry or a comparison result that RISC-V has no flag register for.

---

## 2. Phase 1, step by step

**Setup.** Branched `parity-llvm` from `upstream/release/22.x` (LLVM 22.1.8, close enough to
revive's 22.1.5 for `llvm-sys 221`), built through revive's own builder using a build root whose
`llvm` is a symlink -- the builder resolves `./llvm/`, `./patches/llvm/` and `./target-llvm/`
relative to the working directory, so this avoids duplicating a multi-GB tree.

### Step 0 -- put wide values in the vector registers

`i256` is added to `VRM2`'s type list, so a wide value *is* an LMUL=2 vector register pair -- exactly
256 bits at VLEN=128. The extension implies `Zve64x` and `Zvl128b`, because it genuinely needs those
registers.

This is the foundation everything else sits on, and it is deliberately the vector registers rather
than a file of the extension's own. The allocator knows these operands are vector registers, so a
wide value and an RVV value can never be assigned the same physical register. Everything else reuses
what RVV already has: `VMV2R_V` for copies, `VS2R_V`/`VL2RE8_V` for spills and reloads, `ArgVRM2s`
for arguments. `RISCVInstrInfo.cpp` needs no changes at all, and the register definition is a
two-line addition to a type list.

It also forces the frame-pointer bug to be fixed rather than worked around. With real vector
registers the `emitPrologue` assertion fires immediately, because RVV's stack objects are created
during register allocation -- after `getReservedRegs` has decided whether to reserve `X8`.
`getReservedRegs` now reserves it up front when the extension is on, rather than depending on a
decision that is still allowed to change.

The cost is that RVV spill slots are **scalable**: sized in `vlenb`, so each access needs a runtime
VLEN query and arithmetic. A spill-heavy test emits 37 `csrr` instructions. Non-leaf functions also
carry frame-pointer setup and stack realignment. `-riscv-v-vector-bits-min=128` does not help --
see §3 and §7.

### Step 1 -- the instruction set and the ABI

`addRegisterClass(MVT::i256, ...)` so type legalization stops splitting wide values; operation
actions marking the supported operations Legal and the rest Expand; the instructions themselves in
the custom-2 opcode space; a `Select_VRM2` pseudo for wide selects (2,036 `phi i256` in the corpus);
and wide arguments in vector registers in *both* `CC_RISCV` and `CC_RISCV_FastCC` -- the latter
matters because revive gives its internal functions `fastcc`.

```text
add i256                              icmp ult i256
--------------------------------      ------------------------------
  ld   a5, 0(a2)                        ld    a2, 8(a1)
  ld   s0, 8(a2)                        ld    t1, 16(a1)
  ld   t1, 16(a2)          30 instrs    ld    a3, 24(a1)        22 instrs
  ld   t0, 24(a2)                       beq   a4, a3, .LBB1_3
  ...                                   sltu  t0, a4, a3
                                        ...
->                                    ->
  revive.wadd v8, v8, v10   3 instrs    revive.wsltu a0, v8, v10  3 instrs
```

The ABI change is the larger half. A three-argument wide call passed everything by reference:

```text
call3:  addi sp, sp, -192        ->    call3:  addi sp, sp, -16
        sd   ra, 184(sp)                       sd   ra, 8(sp)
        sd   s0, 176(sp)                       addi s0, sp, 16
        sd   s1, 168(sp)                       andi sp, sp, -16
        addi s0, sp, 192                       call opaque
        andi sp, sp, -16                       addi sp, s0, -16
        ld   a4, 0(a1)                         ld   ra, 8(sp)
        sd   a4, 24(sp)                        addi sp, sp, 16
        ...            56 instrs               ret          9 instrs
```

A 192-byte frame and limb-by-limb marshalling becomes a 16-byte frame and no marshalling at all.

Measured on its own -- wide constants still coming from the pool, which step 2 fixes:

| benchmark | baseline | step 1 | delta |
|---|--:|--:|--:|
| V5QuoteVerifier | 575,524 | 553,100 | -3.90% |
| XENCrypto | 479,234 | 340,772 | **-28.89%** |
| FiatTokenV1 | 373,746 | 375,660 | +0.51% |
| SnapshotPCCSRouter | 306,187 | 302,263 | -1.28% |
| AutomataDcapAttestationFee | 290,231 | 286,901 | -1.15% |
| Festival | 189,194 | 171,942 | -9.12% |
| P2PMarket | 136,095 | 101,139 | -25.68% |
| T3rminalDriver | 134,985 | 102,883 | -23.78% |
| TetherToken | 52,163 | 44,699 | -14.31% |
| ERC20Factory | 25,486 | 22,862 | -10.30% |
| DaimoP256Verifier | 22,045 | 15,609 | **-29.19%** |
| ERC20 | 21,629 | 18,461 | -14.65% |
| Multicall3 | 21,367 | 21,717 | +1.64% |
| WETH9 | 20,436 | 19,816 | -3.03% |
| Sha256 | 8,059 | 11,195 | **+38.91%** |
| **TOTAL** | **2,656,381** | **2,389,019** | **-10.06%** |

**Code falls 26.79% at this step, but the constant pool grows 573%**, so only 10 points survive into
the combined figure. Three benchmarks come out worse. That is not a property of the instruction set;
it is the constant handling, which step 2 addresses.

### Step 2 -- constant materialization

The first working version sent every immediate wider than 11 bits to the constant pool: a 32-byte
`.rodata.cst32` entry plus two instructions to address and load it, where the limb expansion had
built each 64-bit limb with `li`/`lui` and folded away limbs that were zero.

Values that fit an XLen register are now built there and widened. Both directions matter: masks like
`-1` have 256 active bits but only one *significant* bit, and are everywhere in EVM code.

```text
i256 0xFFFFFFFF                       i256 0            i256 -1
------------------------------        --------------    ------------------
  lui  a0, %hi(.LCPI2_0)                (pool load)       (pool load)
  revive.wld v8, %lo(.LCPI2_0)(a0)
  + 32 bytes of .rodata               ->                ->
->                                      revive.wzext      li   a0, -1
  li   a0, -1                           v8, zero          revive.wsext v8, a0
  srli a0, a0, 32
  revive.wzext v8, a0
```

The widening uses target DAG nodes (`RISCVISD::WIDE_ZEXT`/`WIDE_SEXT`) rather than
`ZERO_EXTEND`/`SIGN_EXTEND`, because an extend of a constant folds straight back into a wide constant
and lands right back in the pool. Returning `SDValue()` from the Custom lowering does not work
either -- LegalizeDAG then falls back to its default constant expansion, which *is* the pool.

| benchmark | step 1 | step 2 | gain |
|---|--:|--:|--:|
| V5QuoteVerifier | -3.90% | **-23.14%** | +19.2 pts |
| XENCrypto | -28.89% | **-41.14%** | +12.3 pts |
| FiatTokenV1 | +0.51% | **-17.39%** | +17.9 pts |
| SnapshotPCCSRouter | -1.28% | **-20.34%** | +19.1 pts |
| AutomataDcapAttestationFee | -1.15% | **-19.43%** | +18.3 pts |
| Festival | -9.12% | **-21.85%** | +12.7 pts |
| P2PMarket | -25.68% | **-35.69%** | +10.0 pts |
| T3rminalDriver | -23.78% | **-35.50%** | +11.7 pts |
| TetherToken | -14.31% | **-25.14%** | +10.8 pts |
| ERC20Factory | -10.30% | **-22.85%** | +12.6 pts |
| DaimoP256Verifier | -29.19% | **-37.33%** | +8.1 pts |
| ERC20 | -14.65% | **-27.62%** | +13.0 pts |
| Multicall3 | +1.64% | **-13.62%** | +15.3 pts |
| WETH9 | -3.03% | **-14.76%** | +11.7 pts |
| Sha256 | +38.91% | **+16.28%** | +22.6 pts |
| **TOTAL** | **-10.06%** | **-25.96%** | **+15.9 pts** |

The constant pool drops from +573.53% to +116.46%, and every benchmark improves by 8 to 23 points.
The three that step 1 made worse are all recovered; Sha256 is still a regression but less than half
of what it was, for the reasons in §7.

Both steps can be measured with one compiler: `-riscv-revive-wide-const-in-reg=false` restores the
step-1 behaviour.

### Step 3 -- known bits: implemented, measured, reverted

`computeKnownBitsForTargetNode` for the widening nodes, so the combiner could see through a
materialized constant. It produced **no size change whatsoever** -- eleven modules compared
byte-for-byte identical -- while causing a severe compile-time regression: roughly a quarter of the
corpus began exceeding the harness's 120-second limit, and the largest module went from 1 second to
over 120.

The null result is the expected one in hindsight. The wide arithmetic selects from *generic* ISD
nodes -- `and`, `or`, `shl`, `icmp` -- whose known-bits support is width-agnostic and was never
missing. The only opaque nodes are the two widening ones, and they carry constants the combiner
already knows before legalization. It added recomputation without adding information. What is
actually needed is a *narrowing* transform; see §6.

### A future improvement: a register file of the extension's own

An earlier revision kept `i256` in a separate `W0`-`W15` class, encoded as the even vector register
numbers so the emitted ELF still named vector registers, but modelled inside LLVM as its own class
with fixed-size spill slots. It is **15.6 percentage points smaller**:

| | code | constant pool | combined |
|---|--:|--:|--:|
| vector registers (current) | 1,806,612 | 160,231 | **1,966,843 (-25.96%)** |
| separate `W` file | 1,392,900 | 160,231 | **1,553,131 (-41.53%)** |

| benchmark | vector regs | `W` file | gap |
|---|--:|--:|--:|
| V5QuoteVerifier | -23.1% | -43.3% | 35.7% |
| XENCrypto | -41.1% | -50.3% | 18.3% |
| FiatTokenV1 | -17.4% | -31.8% | 21.0% |
| SnapshotPCCSRouter | -20.3% | -40.0% | 32.9% |
| AutomataDcapAttestationFee | -19.4% | -39.5% | 33.2% |
| Festival | -21.8% | -32.9% | 16.5% |
| P2PMarket | -35.7% | -47.6% | 22.8% |
| T3rminalDriver | -35.5% | -48.2% | 24.6% |
| TetherToken | -25.1% | -37.7% | 20.1% |
| ERC20Factory | -22.9% | -35.9% | 20.4% |
| DaimoP256Verifier | -37.3% | -55.2% | 39.8% |
| ERC20 | -27.6% | -41.2% | 23.0% |
| Multicall3 | -13.6% | -38.8% | 41.1% |
| WETH9 | -14.8% | -27.9% | 18.3% |
| Sha256 | +16.3% | -12.5% | 32.9% |
| **TOTAL** | **-26.0%** | **-41.5%** | **26.6%** |

Almost all of the 413,712-byte gap is the scalable spill slots: `csrr vlenb` sequences that a
fixed-size class does not need.

**It is not adopted, and should not be without more work.** LLVM would not know the two files
overlap, so a wide value and an RVV value could be assigned the same physical register with no
conflict detected -- safe only while no RVV code is generated, which is not a property to rely on
with `Zvl128` enabled. The cheaper route to the same bytes is to keep the vector registers and make
their spill slots fixed-size at a pinned VLEN; see §7.

---

## 3. Phase 2: VLEN width, and VLA against VLS

Both comparisons are null results, measured over all 103 modules on the vector-register design. The
extension implies `Zve64x` and `Zvl128b`, so plain `XReviveVec` is already `Zvl128` and
length-agnostic; the other three raise VLEN or pin it with `-riscv-v-vector-bits-min`.

| configuration | code | constant pool | combined | vs Zvl128 VLA |
|---|--:|--:|--:|--:|
| `Zvl128`, VLA | 1,806,612 | 160,231 | 1,966,843 | baseline |
| `Zvl128`, VLS | 1,806,612 | 160,231 | 1,966,843 | **+0 (0.00%)** |
| `Zvl256`, VLA | 1,806,656 | 160,231 | 1,966,887 | +44 (0.00%) |
| `Zvl256`, VLS | 1,806,656 | 160,231 | 1,966,887 | +44 (0.00%) |

**Zvl128 against Zvl256: 44 bytes in 1.97 MB**, or 0.002%, and `Zvl256` is the *larger* of the two.
Code size saturates well below 256 bits, matching the earlier VLEN sweep that found 256, 512, 1024
and 2048 within 48 bytes of one another.

**VLA against VLS: byte-identical**, at both widths. `-riscv-v-vector-bits-min` changes nothing on
top of `Zvl*b`, which already pins the width -- the flag is redundant rather than an alternative.
The earlier RVV experiment reached the same conclusion at 256; this confirms it at 128 and with the
extension present.

There is a structural reason the second comparison cannot show much: **the extension's instructions
are fixed-width by construction and read neither `vtype` nor `vl`**. Nothing about a `revive.wadd`
is length-agnostic or length-specific. The axis can only reach whatever ordinary RVV codegen the
vectorizer emits alongside them -- which, per the earlier experiment, is byte-compare and
small-memcpy idioms and no vector arithmetic at all. This is worth stating plainly rather than
letting a null result read as "no difference found": the question is close to unanswerable for this
design.

The result that *would* have been useful is negative too. VLS does not make RVV spill slots
fixed-size -- a spill-heavy test emits the same 37 `csrr` instructions either way -- so pinning VLEN
does not recover any of the scalable-stack overhead described in §2. Getting that back needs a
change to how `VRM2` slots are sized, not a command-line flag. Why the flag does not reach the frame
lowering is the open question in §7, and the next thing to establish.

**Conclusion.** Neither knob is worth turning. `Zvl128` is the right setting: it is what the
extension already implies, it is 44 bytes smaller than `Zvl256`, and VLS buys nothing at either
width.

---

## 4. Phase 3: what the extension does not cover

Every instruction in the corpus whose operands are wider than an XLen register -- 72,055 of them
across the 103 modules -- classified by whether the extension selects it to a single instruction.

| category | count | share |
|---|--:|--:|
| Selected to one instruction | 53,089 | 73.7% |
| revive runtime calls carrying wide values | 18,175 | 25.2% |
| **Not covered** | **694** | **1.0%** |
| Instruction defined, but resolc still calls `stdlib.ll` | 97 | 0.1% |

**The arithmetic is essentially fully covered.** What is left is 694 operations, and only two kinds
of them matter.

### Not covered

| operation | width | count |
|---|--:|--:|
| `llvm.umin` | i256 | 463 |
| `__mul` (see below) | i256 | 219 |
| `llvm.umax` | i256 | 4 |
| `zext` / `trunc` / `mul` / `lshr` | i512 | 6 |
| `llvm.umin` | i128 | 2 |

**`umin`/`umax` are the only genuine instruction gap**, at 467 sites. They are `Expand` today, which
produces a compare, a branch and a copy:

```text
revive.wsltu a0, v8, v10
bnez         a0, .LBB0_2
vsetivli     zero, 1, e8, m1, ta, ma
vmv2r.v      v8, v10
```

Five instructions including a `vsetivli`, where a `revive.wminu` would be one. Worth adding, though
at 467 sites the total is small.

The i512 handful is not worth an instruction: eight sites corpus-wide, all from `mulmod`'s exact
product.

### `__mul`: an outlining decision that the extension invalidated

`__mul` is not a stdlib routine. It is a revive-generated wrapper whose entire body is one
`mul i256`, marked `noinline` with `"noinline-oz"`, present in 25 modules with **219 call sites**.

revive outlines it because a `mul i256` used to expand to 204 bytes, so paying a call to share one
copy was a clear win. It is now **one instruction**, and the outlining inverted: every site pays a
call sequence plus the callee's prologue and return in order to execute a single `revive.wmul`.

Inlining it is a revive-side change, not an LLVM one. It is the only such helper -- of the two
`noinline-oz` functions in the corpus, `__mul` is the only one small enough to have flipped -- but
it is worth checking whenever the cost of a wide operation changes by two orders of magnitude, and
it is a reminder that the frontend's size heuristics are calibrated against the old expansion.

### The 25% that are runtime calls

`__revive_store_heap_word` (10,904), `__revive_load_heap_word` (5,370) and
`__revive_load_storage_word` (1,647) dominate this group. They are memory operations rather than
arithmetic, and they are the single largest consumer of wide values in the corpus: 17,921 calls,
more than every wide arithmetic operation in the corpus except `icmp ult`.

Wide memory access is what `wld` and `wst` cover, so the heap word helpers collapse to one of those
plus a byte reversal: ten instructions for `__revive_load_heap_word` and thirteen for its store
twin. That needs `+unaligned-scalar-mem` as well as the extension, because an EVM heap word is a
256-bit access at an alignment of one, which the code generator otherwise splits into 32 byte
accesses reassembled with shifts and ORs -- 132 instructions for the load helper.

---

## 5. What is implemented

`i256` is a legal type with its own register class, so type legalization never splits it.

**Registers.** `i256` is a type `VRM2` holds, so a wide value is an LMUL=2 vector register pair --
exactly 256 bits at VLEN=128. Arguments use the existing `ArgVRM2s`, so `v8` to `v23` -- eight pairs
-- carry them, and a decoder reading the ELF sees vector register numbers. Copies, spills and
reloads are the extension's own `revive.wmv`, `revive.wst` and `revive.wld`, because the standard
whole-register forms need the vector extensions the extension does not imply. An earlier revision
used a parallel register file for this; see the future-improvement note at the end of §2.

**Instructions**, all in the custom-2 opcode space. `funct3` groups the register forms by operand
shape and `funct6` names the operation within a group, with the top bit of `funct7` carrying the
width; the memory forms have no `funct6`, so they take a `funct3` per width and direction:

| group | instructions |
|---|---|
| arithmetic | `wadd` `wsub` `wmul` `wand` `wor` `wxor` `wdivu` `wdiv` `wremu` `wrem` |
| shifts | `wsll` `wsrl` `wsra` (amount in a GPR) |
| compare | `wseq` `wsne` `wsltu` `wslt` (result in a GPR) |
| move/convert | `wmv` `wtrunc` `wzext` `wsext` `wbswap` `wcpop` `wclz` `wctz` |
| memory | `wld` `wst` |
| EVM-specific | `waddmod` `wmulmod` `wexp` `wsignextend` |

`ugt`/`sgt` need no encoding: they are the same instruction with operands swapped. `le`/`ge` forms
are reached by inverting, via `setCondCodeAction(..., Expand)`.

**Calling convention.** Wide values pass in wide registers in both `CC_RISCV` and `CC_RISCV_FastCC`;
the latter matters because revive gives its internal functions `fastcc`, so that is the path most
`i256` arguments actually take. A three-argument wide call now marshals nothing at all:

```text
f_call:  addi sp, sp, -8 ; sd ra, 0(sp) ; call opaque ; ld ra, 0(sp) ; addi sp, sp, 8 ; ret
```

against 144 bytes of address arithmetic and copies before. Single operations collapse accordingly:
`add i256` from 28 instructions to `revive.wadd v8, v8, v10`, and `udiv i256` from 1,040 bytes to
one instruction.

**Supporting work**: a `Select_VRM2` pseudo for wide selects (2,036 `phi i256` in the corpus),
constant materialization -- XLen-sized values built in a GPR and widened through target nodes,
anything genuinely wider from the constant pool -- hand-written lowering for 128-bit loads and
stores, and reserving the frame pointer up front so RVV's late stack objects cannot invalidate the
`hasFP` decision.

---

### Tests

`llvm/test/CodeGen/RISCV/xrevivevec-*.ll` -- eight files, 542 checks, generated with
`update_llc_test_checks.py` and idempotent, so the assertions are what the compiler actually emits:

| file | covers |
|---|---|
| `arith` | every wide arithmetic and shift, with both narrowed and wide shift amounts |
| `cmp` | all ten predicates, plus a compare feeding a branch |
| `mem` | full-width, extending and truncating access, i128, and the conversions |
| `const` | zero, small, 32- and 64-bit masks, `-1`, a genuinely wide value, a constant operand |
| `abi` | argument passing and calls on RV64I and RV64E, fastcc, register exhaustion, live-across-call |
| `select` | select, wide-condition select, phi, and enough live values to force spilling |
| `intrinsics` | `addmod`, `mulmod`, `exp`, `signextend`, `bswap` |
| `disabled` | with the feature off, i256 expands as before and nothing leaks in |

`llvm/test/MC/RISCV/xrevivevec-disasm.txt` covers the decoder for 27 encodings byte-exactly, and
asserts each is invalid without the extension.

Two things came out of writing them. **A real bug**: `sextload i8/i16/i32 -> i256` could not be
selected, because `sign_extend_inreg` is formed after legalization and never reached the Custom
lowering; it is now matched at selection. The corpus never exercised it, which is the argument for
having the tests.

**A gap left open**: the assembler cannot parse `revive.wadd v8, v10, v12`, because the parser does
not accept a VRM2 operand written as `v8` -- no upstream RVV instruction has one. Codegen and
disassembly both work, so this only matters for hand-written assembly, and the MC test is scoped to
the decoder accordingly.

---

## 6. What went wrong

Eight issues were found and fixed. Most were not in the new code but in **existing code that assumed
no scalar type could be wider than an XLen register**.

1. **`riscv_seteq` is a C++ `ComplexPattern`**, not a TableGen pattern, so it never type-checked its
   operands. It matched `setcc` on `i256` and emitted a GPR `XOR`, silently inserting a cross-class
   copy. Guarded on the operand being XLen-sized. The same trap was hit by the earlier `w256`
   prototype: a C++ matcher is invisible to TableGen's type inference.

2. **`DAGCombiner` merged adjacent constant stores into a 256-bit store, then re-widened its own
   result forever.** This hung the largest module. Fixed by overriding `canMergeStoresTo`, which is
   the correct answer anyway: a merged wide constant store needs a constant-pool entry and a load,
   where the unmerged form used plain immediate stores.

3. **`ExpandIRInsts` rewrote wide `udiv`/`urem` into libcalls before they reached the selector**,
   because `MaxDivRemBitWidthSupported` was 128. Raised to 256 when the extension is on.

4. **The calling-convention fast path desynchronised `PendingLocs`** for split aggregates, and the
   rejection surfaced as `llvm_unreachable(nullptr)`, an abort with no message at all. Guarded on
   `!ArgFlags.isSplit() && PendingLocs.empty()`.

5. **Wide spill slots requested 32-byte alignment**, which exceeds the 8-byte stack alignment of
   `LP64E`. The register allocator creates those slots *after* `getReservedRegs` has decided whether
   to reserve the frame pointer, so `emitPrologue` then found `hasFP` true with `X8` unreserved and
   asserted. These loads and stores carry no alignment requirement of their own, so the slot
   alignment was dropped to the stack alignment.

6. **Missing patterns surfaced one at a time** as the corpus exercised them: `bswap` (no generic
   expansion exists at this width, and EVM byte-swaps constantly for big-endian calldata),
   extending loads from `i8`/`i16`/`i32`/`i64`, and truncating stores. `EXTLOAD` cannot be marked
   `Expand`, because LegalizeDAG asserts it is always supported, so it needs real patterns. The missing
   `extloadi64` pattern was what made the largest module pathologically slow.

7. **Every wide immediate went to the constant pool**, because the first version only built values
   of 11 bits or fewer in a register. This cost more than it saved and made Sha256 look like a code
   regression when its code had shrunk. Fixed with target widening nodes; see §2, step 2.

8. **The disassembler needed a decoder for the new register class**, and TableGen generates a call
   to it whether or not one exists. This only appeared on a full build: iterating with `ninja llc`
   never compiles the disassembler, so the first `ninja install` failed on a target that had been
   silently broken for hours. Worth knowing for anyone iterating the same way.

---

## 7. The biggest thing left on the table

**Fixed-size spill slots for `VRM2` at a pinned VLEN.** This is worth 413,712 bytes -- the whole gap
between the design measured here and the separate register file measured at the end of §2 -- and
unlike that alternative it costs no correctness. It is what the extension does now: the backend
states the 128-bit register width itself and spills with `revive.wst` and `revive.wld` against plain
32-byte stack objects.

The gap isolates cleanly: **all of it is code.** The constant pool is byte-identical between the two
designs (160,231 either way), so nothing here is about data.

The mechanism is visible in a single spill:

```text
csrr a1, vlenb        <- RVV slots are sized in vlenb, not bytes
li   a2, 38
mul  a1, a1, a2
add  a1, a1, sp
addi a1, a1, 32
vs2r.v v10, (a1)
```

Five instructions of address arithmetic before the store. A fixed 32-byte slot needs none of them:
one `wst` at an immediate offset. The corpus performs **24,113** whole-register spills and reloads.

Measured over all 103 modules:

| category | instructions | bytes | of the gap |
|---|--:|--:|--:|
| scalable spill addressing | 35,395 | 141,580 | 34% |
| frame-pointer setup and teardown | 5,635 | 22,540 | 5% |
| `vsetvli` around vector operations | 2,516 | 10,064 | 2% |
| **attributed** | **43,546** | **174,184** | **42%** |

**Only 42% is attributed, and the remainder is not yet explained.** The gap implies roughly 103,000
extra instructions against about 43,500 identified. Most of the shortfall is likely the measurement
itself: address arithmetic is counted only when it sits immediately before its spill, so anything
hoisted or interleaved is missed -- the sequence above is five instructions per spill where the
walk-back averages 1.5.

One hypothesis was tested and rejected: reserving `X8` does not cause scalar spilling (zero
callee-saved GPR spills in the spill test), so RV64E register pressure is not the cause.

Implementing the fix is also what settles the attribution. If the gap closes to near zero, the
accounting above was right; if it does not, the residual is register-allocation quality.

### Why VLS does not help

`-riscv-v-vector-bits-min=128` on top of `Zvl128b` changes nothing -- byte-identical output, and the
same 37 `csrr` in the spill test (§3) -- where on the face of it VLS is exactly the mechanism that
should make a `VRM2` slot a known 32 bytes.

What the flag actually does is make fixed-length *vector types* legal, so `<4 x i64>` and friends can
be used instead of scalable ones. It does not appear to change how `RISCVFrameLowering` sizes or
addresses the RVV stack region, which stays expressed in `vlenb` regardless. The fix landed by
another route: no vector type is legal, so a wide value never reaches that stack region, and the
extension's own spill instructions address fixed-size slots directly.

**Narrowing wide operations whose result only needs XLen bits.** EVM contracts hold `uint32`,
`uint64` and `address` values inside 256-bit words, and once `i256` is a machine type every one is
computed at full width. The limb expansion got the demotion for free: masking against `0xFFFFFFFF`
folded the upper three limbs to literal zeros and the knowledge propagated downstream.

The obvious-looking fix -- `computeKnownBitsForTargetNode` -- was implemented and measured and does
**not** work; see §2, step 3. Known bits were never the missing ingredient. What is needed is a
*narrowing* transform: when the demanded bits of a wide operation fit an XLen register, replace it
and its operands with the scalar instruction. That is where the residual Sha256 regression lives.

---

## 8. The EVM-specific instructions

`waddmod`, `wmulmod`, `wexp` and `wsignextend` have encodings, intrinsics and patterns, each selects
to a single instruction, and resolc emits the intrinsics rather than calling the hand-written
routines in `stdlib.ll`:

```text
revive.waddmod v8, v8, v10, v12      revive.wexp        v8, v8, v10
revive.wmulmod v8, v8, v10, v12      revive.wsignextend v8, v8, v10
```

The saving is the routine bodies becoming unreferenced. The corpus has 57 `__mulmod`, 22 `__addmod`
and 18 `__exp` call sites; `__mulmod` is 4,534 bytes and drags in `__ulongrem` at 5,710 for the slow
path that every modulus near 2²⁵⁶ takes, and the production routines plus helpers come to 15,688
bytes. Only the PVM linker garbage-collects them, so the `llc` figures in this chapter **exclude**
this win entirely; the blob figures at the top of the chapter include it.

Getting there needed one piece of LLVM plumbing worth calling out: **intrinsics could not take a
256-bit scalar**. The type encoding LLVM uses for intrinsic signatures stopped at `IIT_I128`,
because no in-tree target has a scalar that wide, so the declarations verified as "incorrect return
type". Adding `IIT_I256` and its decoder case fixed it.

A compiled object is dense with the wide instructions: ERC20's `.text` disassembles to 387 `wld`,
180 `wst`, 52 `wtrunc`, 47 `wsrl`, 45 `wsltu`, 41 `wseq`, 37 `wadd`, 32 `wbswap`.

---

## 9. What could go wrong

**The register overlap hazard is resolved.** An earlier revision kept the wide values in a parallel
register file that overlapped the vector registers without LLVM knowing, so the allocator could
assign a wide value and an RVV value to the same physical register. `i256` now lives on `VRM2`
itself, so no such aliasing exists. The cost of that correctness is measured at the end of §2.

Worth recording that the reasoning which produced the parallel file was partly wrong. A class that
*aliases* the vector registers does break vector copies and spills -- 424 of 2,595 RVV tests in the
earlier prototype -- but adding `i256` to `VRM2`'s type list is a different thing and works, despite
`VReg` sizing the class from ELEN. That possibility was dismissed by inference rather than tested.

**The wide registers are no longer all caller-saved.** Half the file -- `v0` to `v7` and `v24` to
`v31` -- is callee saved, where the standard vector convention preserves none, so a value live
across a call is saved once by the callee rather than at every call site.

**Compile time.** Two pathologies were found and fixed -- the store-merging loop and the missing
`extloadi64` pattern -- and every module now compiles inside the harness's 120-second limit, where
the largest previously did not. Whole-contract `resolc` runs are still slow: `AutomataDcapAttestationFee`
ran 12 CPU-minutes past codegen without completing its link when this was last measured, before the
constant fix. Worth re-checking and profiling before this goes near CI.

**Semantics are verified by the test suites rather than by the figures here.** Every result in this
chapter is a size measurement. Correctness comes from the extension being on by default: the
integration and differential suites compile every contract through the wide instructions and assert
the same state changes as the EVM.

---

## 10. How these were measured

Every figure above is `.text` and `.rodata` of the compiled object, taken per section with
`llvm-size -A`. The default Berkeley `text` column folds `.rodata` in with the code, which is what
hid the constant-pool growth on the first pass and made Sha256 look like a code regression when its
code had shrunk 23%.

The inputs are the post-optimization LLVM IR of the 15 benchmark contracts, dumped with
`resolc --debug-output-dir` and compiled with `llc`. Working from the dumped IR keeps the comparison
to one variable -- the target features -- and avoids rebuilding the compiler for every backend
change.

One trap is worth recording: revive stamps an explicit `target-features` attribute on some
functions, and a per-function attribute beats `-mattr` on the command line. In a typical module only
5 of 15 attribute groups carry one, and the functions doing the `i256` work are not among them. The
features have to be rewritten inside the IR *and* passed on the command line; passing either alone
silently measures nothing, which produced a byte-identical "result" on the first run.

`-riscv-revive-wide-const-in-reg=false` restores the step-1 constant handling, so steps 1 and 2 can
be compared with a single compiler.

The scripts that produced these numbers are not part of the repository.
