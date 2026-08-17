# Wide integer extension: measurements

Phase 1 of the vector-extension experiment. The goal was to stop LLVM legalizing EVM words into
`i64` limb chains, and instead select one instruction per wide operation with operands living in
wide registers.

Measured on the 15 benchmark contracts under `benchmarks/single-file` (103 modules), the same
corpus used for the earlier carry-extension analysis, which established where revive's code size
goes.
Sizes are taken per section from the compiled object -- code and constant pool separately, because
the extension moves them in opposite directions. PVM blob sizes are unavailable because the polkavm
linker cannot decode the new encodings, as agreed for this phase.

---

## 1. Result

**−909,658 bytes, −35.29%** across the 15 benchmark contracts (103 modules). Every contract
improves. Code and read-only data move in opposite directions and are reported separately:

| | baseline | XReviveVec | delta |
|---|--:|--:|--:|
| **code** (`.text`) | 2,503,468 | **1,507,562** | **−995,906 (−39.78%)** |
| constant pool (`.rodata`) | 74,015 | 160,263 | +86,248 (+116.53%) |
| **combined** | **2,577,483** | **1,667,825** | **−909,658 (−35.29%)** |

Wide values live in the vector registers -- `i256` is a type `VRM2` holds, so at VLEN=128 a wide
value is an LMUL=2 register pair. All 103 modules compile.

For scale, the carry extension measured −8.64% on this corpus, and did not change the argument ABI.

### Per benchmark

| benchmark | modules | baseline | combined | code only |
|---|--:|--:|--:|--:|
| V5QuoteVerifier | 18 | 565,562 | −37.69% | −41.66% |
| XENCrypto | 10 | 427,602 | −39.94% | −44.07% |
| FiatTokenV1 | 16 | 371,048 | −26.94% | −32.80% |
| SnapshotPCCSRouter | 13 | 300,473 | −35.14% | −40.54% |
| AutomataDcapAttestationFee | 17 | 284,507 | −34.25% | −39.29% |
| Festival | 14 | 188,814 | −27.82% | −32.52% |
| P2PMarket | 1 | 135,657 | −42.45% | −44.65% |
| T3rminalDriver | 3 | 133,685 | −43.01% | −45.41% |
| TetherToken | 3 | 51,145 | −30.88% | −34.82% |
| ERC20Factory | 2 | 25,474 | −30.58% | −34.37% |
| DaimoP256Verifier | 1 | 22,051 | **−51.71%** | −57.28% |
| ERC20 | 1 | 21,617 | −35.53% | −38.91% |
| Multicall3 | 1 | 21,363 | −33.21% | −35.87% |
| WETH9 | 2 | 20,426 | −23.07% | −26.90% |
| Sha256 | 1 | 8,059 | −7.97% | −15.69% |
| **TOTAL** | **103** | **2,577,483** | **−35.29%** | **−39.78%** |

Sha256 improves least: it is a 32-bit algorithm written in 256-bit words, so it gains least from
wide instructions while still paying for the constant pool. See §7.

**On corpus freshness.** These figures come from IR dumped with the compiler at the time of writing.
An earlier revision of this analysis used dumps that were ten days old, and the drift mattered: one
of the gaps it identified (`__mul`, below) had already been fixed in revive, and one that current
revive does emit (`llvm.umul.with.overflow.i256`) was absent entirely. Measurements that compile a
fixed corpus with two compilers are unaffected by staleness -- the input is the controlled variable
-- but any claim about *what revive emits* has to be re-dumped before it is trusted.

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
256 bits at VLEN=128. The feature implies `Zve64x` and `Zvl128b`, because it needs those registers.

This is deliberately the vector registers rather than a file of the extension's own. The allocator
then knows these operands overlap ordinary vector values, so a wide value and an RVV value can never
share a physical register. Everything else reuses what RVV has: `VMV2R_V` for copies,
`VS2R_V`/`VL2RE8_V` for spills, `ArgVRM2s` for arguments. `RISCVInstrInfo.cpp` needs no new copy or
spill code, and the register definition is a two-line addition to a type list.

The cost, at this point, is that RVV spill slots are **scalable**: sized in `vlenb`, so each access
needs a runtime VLEN query and arithmetic. A spill-heavy test emits 37 `csrr`. Non-leaf functions
also carry frame-pointer setup and stack realignment. Step 4 removes both.

### Step 1 -- the instruction set and the ABI

`addRegisterClass(MVT::i256, ...)` so type legalization stops splitting wide values; operation
actions marking the supported operations Legal and the rest Expand; the instructions themselves in
the custom-2 opcode space; a `Select_VRM2` pseudo for wide selects (2,036 `phi i256` in the corpus);
and wide arguments in vector registers in *both* `CC_RISCV` and `CC_RISCV_FastCC` -- the latter
matters because revive gives its internal functions `fastcc`.

```
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

```
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
| V5QuoteVerifier | 575,524 | 553,100 | −3.90% |
| XENCrypto | 479,234 | 340,772 | **−28.89%** |
| FiatTokenV1 | 373,746 | 375,660 | +0.51% |
| SnapshotPCCSRouter | 306,187 | 302,263 | −1.28% |
| AutomataDcapAttestationFee | 290,231 | 286,901 | −1.15% |
| Festival | 189,194 | 171,942 | −9.12% |
| P2PMarket | 136,095 | 101,139 | −25.68% |
| T3rminalDriver | 134,985 | 102,883 | −23.78% |
| TetherToken | 52,163 | 44,699 | −14.31% |
| ERC20Factory | 25,486 | 22,862 | −10.30% |
| DaimoP256Verifier | 22,045 | 15,609 | **−29.19%** |
| ERC20 | 21,629 | 18,461 | −14.65% |
| Multicall3 | 21,367 | 21,717 | +1.64% |
| WETH9 | 20,436 | 19,816 | −3.03% |
| Sha256 | 8,059 | 11,195 | **+38.91%** |
| **TOTAL** | **2,656,381** | **2,389,019** | **−10.06%** |

**Code falls 26.79% at this step, but the constant pool grows 573%**, so only 10 points survive into
the combined figure. Three benchmarks come out worse. That is not a property of the instruction set;
it is the constant handling, which step 2 addresses.

### Step 2 -- constant materialization

The first working version sent every immediate wider than 11 bits to the constant pool: a 32-byte
`.rodata.cst32` entry plus two instructions to address and load it, where the limb expansion had
built each 64-bit limb with `li`/`lui` and folded away limbs that were zero.

Values that fit an XLen register are now built there and widened. Both directions matter: masks like
`-1` have 256 active bits but only one *significant* bit, and are everywhere in EVM code.

```
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
| V5QuoteVerifier | −3.90% | **−23.14%** | +19.2 pts |
| XENCrypto | −28.89% | **−41.14%** | +12.3 pts |
| FiatTokenV1 | +0.51% | **−17.39%** | +17.9 pts |
| SnapshotPCCSRouter | −1.28% | **−20.34%** | +19.1 pts |
| AutomataDcapAttestationFee | −1.15% | **−19.43%** | +18.3 pts |
| Festival | −9.12% | **−21.85%** | +12.7 pts |
| P2PMarket | −25.68% | **−35.69%** | +10.0 pts |
| T3rminalDriver | −23.78% | **−35.50%** | +11.7 pts |
| TetherToken | −14.31% | **−25.14%** | +10.8 pts |
| ERC20Factory | −10.30% | **−22.85%** | +12.6 pts |
| DaimoP256Verifier | −29.19% | **−37.33%** | +8.1 pts |
| ERC20 | −14.65% | **−27.62%** | +13.0 pts |
| Multicall3 | +1.64% | **−13.62%** | +15.3 pts |
| WETH9 | −3.03% | **−14.76%** | +11.7 pts |
| Sha256 | +38.91% | **+16.28%** | +22.6 pts |
| **TOTAL** | **−10.06%** | **−25.96%** | **+15.9 pts** |

Steps 1 and 2 were measured before the VLEN and frame-pointer work described in step 0, so these
figures show the size of each gain rather than the final numbers in §1.

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

### Step 4 -- VLS was never actually set

Steps 0 to 3 ran with what looked like a vector-length-specific configuration and behaved as though
it were length-agnostic: every RVV spill slot scalable, every access through `vlenb`. §3 had already
found that `-riscv-v-vector-bits-min` changed nothing and recorded it as a null result. It was not a
null result -- the configuration was wrong.

**`Zvl128b` sets a floor, not a width.** That is what the extension means in the RISC-V spec, VLEN
*at least* 128, and LLVM follows it: `getRealMaxVLen()` stays at 65536 unless
`-riscv-v-vector-bits-max` is given, so `getRealVLen()` never yields a known value.
`-riscv-v-vector-bits-min` sets the floor again and moves nothing. **Both** bounds have to be
pinned, and neither `Zvl128b` nor the min flag does that.

**And nothing on the spill path consulted VLEN anyway.** In `RISCVInstrInfo::storeRegToStackSlot`
the decision came from the register class alone:

```cpp
if (RISCVRegisterInfo::isRVVRegClass(RC)) {
  ... TypeSize::getScalable(MFI.getObjectSize(FI)) ...
  MFI.setStackID(FI, TargetStackID::ScalableVector);
```

So even with the width known the slot stayed scalable. VLS governs vector *type* legalization; it
never reached stack object classification.

Three changes follow from that. The feature now pins both VLEN bounds itself, so no flags are
needed. Where the width is known, spill slots are ordinary fixed-size objects:

```
csrr a1, vlenb ; li a2, 38 ; mul a1, a1, a2      addi   a1, sp, 584
add  a1, a1, sp ; addi a1, a1, 32          ->    vs2r.v v10, (a1)
vs2r.v v10, (a1)                                 # 32-byte Folded Spill
```

And `hasRVVFrameObject()` is answered precisely rather than as `hasVInstructions()`. That function
is deliberately imprecise upstream, because scanning stack objects gives different answers before
and after register allocation -- RVV spill slots appear during it (issue 53016). With fixed slots
the extension never creates a scalable object, so the scan is stable. Returning `true` for every
function was forcing 16-byte frame alignment, which exceeds LP64E's 8-byte stack alignment, so every
function realigned and needed a frame pointer. Leaf functions are now `revive.wadd v8, v8, v10 ; ret`.

| | code | constant pool | combined |
|---|--:|--:|--:|
| scalable slots, frame pointer | 1,806,612 | 160,231 | 1,966,843 (−25.96%) |
| fixed slots | 1,543,866 | 160,231 | 1,704,097 (−35.85%) |
| + precise `hasRVVFrameObject` | **1,526,096** | 160,231 | **1,686,327 (−36.52%)** |

Per benchmark, against the same corpus before the step:

| benchmark | before | after | gain |
|---|--:|--:|--:|
| V5QuoteVerifier | 442,362 | 358,040 | **−19.1%** |
| FiatTokenV1 | 308,768 | 273,340 | −11.5% |
| XENCrypto | 282,072 | 258,936 | −8.2% |
| SnapshotPCCSRouter | 243,915 | 198,497 | −18.6% |
| AutomataDcapAttestationFee | 233,851 | 190,649 | −18.5% |
| Festival | 147,862 | 136,552 | −7.6% |
| P2PMarket | 87,525 | 78,491 | −10.3% |
| T3rminalDriver | 87,061 | 76,277 | −12.4% |
| TetherToken | 39,047 | 35,395 | −9.4% |
| ERC20Factory | 19,662 | 17,696 | −10.0% |
| Multicall3 | 18,457 | 14,481 | **−21.5%** |
| WETH9 | 17,420 | 15,738 | −9.7% |
| ERC20 | 15,655 | 13,949 | −10.9% |
| DaimoP256Verifier | 13,815 | 10,849 | **−21.5%** |
| Sha256 | 9,371 | 7,437 | −20.6% |
| **TOTAL** | **1,966,843** | **1,686,327** | **−14.3%** |

**280,516 bytes**, and the gap to the separate register file falls from 413,712 to 133,196. Every
benchmark improves, by 7.6% to 21.5%; Sha256 stops being a regression.

The spread tracks how much each contract spills: the biggest gains are the small contracts whose
frames existed only to hold wide values, and XENCrypto gains least because its wide arithmetic
mostly stays in registers. `check-llvm-codegen-riscv` passes 2454/2454 and
the RVV suite 1200/1200 -- the fixed-slot change is gated on the extension, so ordinary RVV is
untouched.

---

### A future improvement: a register file of the extension's own

An earlier revision kept `i256` in a separate `W0`-`W15` class, encoded as the even vector register
numbers so the emitted ELF still named vector registers, but modelled inside LLVM as its own class
with fixed-size spill slots. It remains smaller:

| | code | constant pool | combined |
|---|--:|--:|--:|
| vector registers (current) | 1,526,096 | 160,231 | **1,686,327 (−36.52%)** |
| separate `W` file | 1,392,900 | 160,231 | **1,553,131 (−41.53%)** |

The gap is **133,196 bytes**, down from 413,712 before the VLEN and frame-pointer fixes, and the
spread across benchmarks is now 7-13% rather than 16-41% -- one systematic remainder rather than
several causes. What is left is most likely allocation quality; see §7.

**It is not adopted, and should not be without more work.** LLVM would not know the two register
files overlap, so a wide value and an RVV value could be assigned the same physical register with no
conflict detected -- safe only while no RVV code is generated, which is not a property to rely on.

---

## 3. Phase 2: VLEN width, and VLA against VLS

Both comparisons are null results, measured over all 103 modules on the vector-register design. The
extension implies `Zve64x` and `Zvl128b`, so plain `XReviveVec` is already `Zvl128` and
length-agnostic; the other three raise VLEN or pin it with `-riscv-v-vector-bits-min`.

| configuration | code | constant pool | combined | vs Zvl128 VLA |
|---|--:|--:|--:|--:|
| `Zvl128`, VLA | 1,806,612 | 160,231 | 1,966,843 | — |
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

### Second pass, after step 4

The measurements above were taken while VLS was not actually in effect (§2, step 4). Repeating them
on the corrected configuration does not refine the comparison -- it removes it. Both axes collapse.

**VLA is no longer a valid configuration.** Forcing it back with `-riscv-v-vector-bits-max=0` makes
the slots scalable again, which reintroduces the 16-byte frame alignment the precise
`hasRVVFrameObject` no longer asks for, and compilation asserts in `emitPrologue`. The extension
requires a pinned VLEN; it is not a tuning choice.

**VLEN=256 is meaningless for this design.** The mapping is `i256` onto an LMUL=2 group, which is
256 bits only at VLEN=128. At 256 the group is 512 bits, so every wide value occupies a slot twice
its size:

```
VLEN=128:  vs2r.v v10, (a1)   # 32-byte Folded Spill
VLEN=256:  vs2r.v v10, (a1)   # 64-byte Folded Spill
```

The spill frame in the same test grows from 616 to 1224 bytes, and half of it is padding. The
extension is defined at VLEN=128 by construction.

Measured properly, the four configurations are no longer equivalent. All carry
`"frame-pointer"="all"`, since VLA needs it and charging that to one side would confound the
comparison:

| configuration | code | constant pool | combined | vs VLA 128 |
|---|--:|--:|--:|--:|
| VLA, VLEN ≥ 128 | 1,819,342 | 160,231 | 1,979,573 | — |
| VLA, VLEN ≥ 256 | 1,819,384 | 160,231 | 1,979,615 | +42 (+0.00%) |
| **VLS, VLEN = 128** | **1,567,822** | 160,231 | **1,728,053** | **−251,520 (−12.71%)** |
| VLS, VLEN = 256 | 1,595,984 | 160,231 | 1,756,215 | −223,358 (−11.28%)  |

Three things fall out.

**VLS against VLA is worth 12.71%** -- the whole scalable-addressing cost from step 4, now visible
as a direct comparison rather than inferred. This is what the first pass should have measured.

**Width still does not matter for VLA**: 42 bytes between 128 and 256, matching the original null
result. With nothing pinned, the code is identical whatever the floor.

**Width does matter for VLS, and 128 wins by 28,162 bytes.** At VLEN=256 an LMUL=2 group is 512
bits, so each wide value occupies a slot twice its size and half the frame is padding. 128 is not
just the natural width for the mapping; it is the smaller one.

So the Phase 2 axes are not both null after all. VLA against VLS is the real choice and VLS wins
decisively; width is a null result only in the length-agnostic mode that the extension cannot use.
For the shipped configuration -- VLS at 128 -- neither is a tuning knob: the width is fixed by the
register mapping, and VLA is incompatible with the fixed-size slots that make the design worth
having.

---

## 4. Phase 3: what the extension does not cover

Every instruction in the corpus whose operands are wider than an XLen register -- 72,055 of them
across the 103 modules -- classified by whether the extension selects it to a single instruction.

| category | count | share |
|---|--:|--:|
| Selected to one instruction | 53,109 | 73.9% |
| revive runtime calls carrying wide values | 18,157 | 25.3% |
| **Not covered** | **522** | **0.7%** |
| Instruction defined, but resolc still calls `stdlib.ll` | 97 | 0.1% |

**The arithmetic is essentially fully covered.** What is left is 694 operations, and only two kinds
of them matter.

### Not covered

| operation | width | count |
|---|--:|--:|
| `llvm.umin` | i256 | 463 |
| `llvm.umul.with.overflow` | i256 | 47 |
| `llvm.umax` | i256 | 4 |
| `zext` / `trunc` / `mul` / `lshr` | i512 | 6 |
| `llvm.umin` | i128 | 2 |

**`umin`/`umax` are the only genuine instruction gap**, at 467 sites. They are `Expand` today, which
produces a compare, a branch and a copy:

```
revive.wsltu a0, v8, v10
bnez         a0, .LBB0_2
vsetivli     zero, 1, e8, m1, ta, ma
vmv2r.v      v8, v10
```

Five instructions including a `vsetivli`, where a `revive.wminu` would be one. Worth adding, though
at 467 sites the total is small.

The i512 handful is not worth an instruction: eight sites corpus-wide, all from `mulmod`'s exact
product.

### An outlining decision the extension invalidated

`__mul` was a revive-generated wrapper around a single `mul i256`, outlined with `noinline` because
the operation cost 458 bytes. With the extension it costs 13, so the call became dearer than the
work. **Current revive no longer emits it**; the gap is closed. It is recorded here because the
heuristic that produced it is still in place, and the same inversion will recur for any helper whose
body becomes one instruction.

### The 25% that are runtime calls

`__revive_store_heap_word` (10,904), `__revive_load_heap_word` (5,370) and
`__revive_load_storage_word` (1,647) dominate this group. They are memory and storage operations,
not arithmetic, so they are not candidates for instructions -- but they are the single largest
consumer of wide values in the corpus, and they benefit from step 0 and step 1 anyway: the value
now arrives in a register pair instead of by reference.

If wide memory access is ever worth an instruction of its own, this is where the volume is: 17,921
calls, more than every wide arithmetic operation in the corpus except `icmp ult`.

---

### Re-checked after step 4

The coverage figures are unchanged: step 4 altered how wide values are spilled and framed, not which
operations have instructions. 73.7% selected, 1.0% not covered, 25.2% runtime calls.

The `umin`/`umax` gap is also unchanged -- still five instructions, and still carrying a `vsetivli`
for what is only a register copy:

```
revive.wsltu a0, v8, v10 ; bnez a0, .LBB0_2 ; vsetivli zero, 1, e8, m1, ta, ma ; vmv2r.v v8, v10
```

That `vsetivli` is the same class of overhead §7 counts at 9,200 bytes corpus-wide: vector
configuration emitted around operations that do not read `vtype`. A `revive.wminu` would remove both
the branch and the copy at 467 sites, and is the one instruction still worth adding.

---

### Prototypes

The three gaps §4 identified were built and measured. All are on the branch; the corpus totals below
are cumulative, since each was measured on top of the previous.

#### 1. Wide min and max

`umin`/`umax` were the only genuine instruction gap: 467 sites, each expanding to a compare, a
branch and a register copy -- and the copy dragged a `vsetvli` with it. Four instructions added
(`wminu`, `wmin`, `wmaxu`, `wmax`), and the four operations moved from Expand to Legal.

```
revive.wsltu a0, v8, v10 ; bnez a0, .LBB0_2      ->   revive.wminu v8, v8, v10
vsetivli zero, 1, e8, m1, ta, ma ; vmv2r.v v8, v10
```

| benchmark | before | after | gain |
|---|--:|--:|--:|
| V5QuoteVerifier | 328,998 | 327,374 | −0.49% |
| XENCrypto | 235,854 | 235,640 | −0.09% |
| FiatTokenV1 | 240,108 | 239,824 | −0.12% |
| SnapshotPCCSRouter | 176,628 | 175,794 | −0.47% |
| AutomataDcapAttestationFee | 170,824 | 169,772 | −0.62% |
| Festival | 122,298 | 122,122 | −0.14% |
| P2PMarket | 74,938 | 74,606 | −0.44% |
| T3rminalDriver | 72,018 | 71,956 | −0.09% |
| TetherToken | 32,256 | 32,216 | −0.12% |
| ERC20Factory | 16,030 | 16,014 | −0.10% |
| DaimoP256Verifier | 9,280 | 9,210 | −0.75% |
| ERC20 | 12,860 | 12,844 | −0.12% |
| Multicall3 | 13,584 | 13,372 | **−1.56%** |
| WETH9 | 14,200 | 14,172 | −0.20% |
| Sha256 | 6,220 | 6,200 | −0.32% |
| **TOTAL (code)** | **1,526,096** | **1,521,116** | **−0.33%** |

**4,980 bytes.** Small, as 467 sites predicted, but cheap: four encodings, no lowering code, and it
removes a branch and a `vsetvli` per site.

#### 2. 512- and 1024-bit widths

`i512` is added to `VRM4` (LMUL=4) and a new `i1024` machine type to `VRM8` (LMUL=8), so the same
instruction set exists at three widths. The operation encoding could not carry the width -- the I-
and S-type memory forms have no spare field -- so **each width takes its own custom opcode**: i256 in
custom-2, i512 in custom-3, i1024 in custom-1. That is three of the four custom opcode spaces, which
is the real cost of this step.

The corpus barely notices, because it contains eight i512 operations in total. An artificial
benchmark -- the same arithmetic, compare, shift and memory work at each width -- shows what the
widths are worth where they are used:

| width | expanded | with the extension | delta |
|---|--:|--:|--:|
| i256 | 3,404 | 88 | **−97.4%** |
| i512 | 10,886 | 88 | **−99.2%** |
| i1024 | 45,328 | 118 | **−99.7%** |

The expansion grows superlinearly -- multiplication is quadratic in limb count -- while the
instruction sequence stays flat, so the wider the type the larger the factor. On the real corpus the
change is within noise, and it is worth having only if contracts start using types wider than a
machine word for their own sake rather than as `mulmod` intermediates.

Two defects surfaced while building it, both of which the corpus caught: truncation between the
wide widths, and zero-extension between them, neither of which existed when there was only one width.

#### 3. Emitting `mul` instead of calling `__mul` -- already done

`__mul` was a revive-generated wrapper whose whole body was one `mul i256`, marked `noinline`, in 25
modules with 219 call sites. It was outlined because the operation used to cost 458 bytes; with the
extension it costs 13, so the call had become more expensive than the work.

**Current revive no longer emits it.** Modelling the change on the older corpus measured −6,492
bytes, but re-dumping showed zero occurrences across all 103 modules: the frontend had already been
fixed. The correct value of this step today is **zero**, and the −6,492 figure is an artefact of
measuring against stale input.

The underlying point still holds for the future. `add_noinline_minsize_attrs` marks outlined helpers
`noinline` for size reasons, and that decision is calibrated against the expansion cost. Any helper
whose body becomes a single instruction inverts the same way, so the heuristic is worth revisiting
whenever the instruction set grows.

Re-dumping also surfaced a gap that the older corpus did not contain at all:
**`llvm.umul.with.overflow.i256`, 47 sites**. Current revive emits checked multiplication as the
overflow intrinsic, which the extension does not cover and which therefore expands. It belongs with
`umin`/`umax` as an instruction candidate.

**Together the prototypes are worth about 5,000 bytes**, once the third is discounted to zero. None of them is where
the remaining money is -- that is still the allocation gap in §7 -- but the first and third are
small, self-contained and have no downside, and the second buys a capability rather than bytes.

---

### Cost of every instruction reaching codegen

Frequency and size are unrelated, and reporting one as if it were the other is a mistake this
analysis made earlier. `unreachable` is 11.7% of the IR and emits **no instruction at all**; `call`
and `br` are the two most frequent opcodes and contribute nothing measurable. Meanwhile
`mul nuw nsw i256` appears once and costs 458 bytes.

So the table below carries both, keyed on the (opcode, type, flags) triple: how often it reaches
codegen across the 103 modules, and what one costs under each compiler. **The product is the only
number that ranks work.**

| instruction | count | base | ext | base total | ext total |
|---|--:|--:|--:|--:|--:|
| `call` | 41,206 | — | — | — | — |
| `br` | 25,254 | — | — | — | — |
| `unreachable` | 20,909 | — | — | — | — |
| `trunc nuw i256→i32` | 6,999 | 10 | 8 | 69,990 | 55,992 |
| `alloca` | 6,957 | — | — | — | — |
| `ptrtoint ptr→i32` | 6,685 | — | — | — | — |
| **`icmp ult i256`** | 6,541 | **73** | **11** | **477,493** | 71,951 |
| `icmp ult samesign i256` | 4,884 | 73 | 11 | 356,532 | 53,724 |
| `store i256` | 4,814 | 35 | 9 | 168,490 | 43,326 |
| `add nuw i32` | 4,493 | 11 | 11 | 49,423 | 49,423 |
| `load i256` | 4,147 | 32 | 8 | 132,704 | 33,176 |
| `load i32` | 3,825 | 10 | 10 | 38,250 | 38,250 |
| `icmp eq i256` | 3,270 | 68 | 19 | 222,360 | 62,130 |
| `and i256` | 3,051 | 62 | 13 | 189,162 | 39,663 |
| `add nuw nsw i256` | 2,678 | 105 | 13 | 281,190 | 34,814 |
| `icmp ult i32` | 2,471 | 13 | 13 | 32,123 | 32,123 |
| `add i256` | 2,371 | 105 | 13 | 248,955 | 30,823 |
| `store i32` | 2,078 | 7 | 7 | 14,546 | 14,546 |
| `phi i256` | 2,048 | — | — | — | — |
| `icmp ugt i256` | 1,488 | 73 | 11 | 108,624 | 16,368 |
| `lshr i256` | 1,058 | 164 | 12 | 173,512 | 12,696 |
| `zext i32→i256` | 999 | 21 | 15 | 20,979 | 14,985 |
| `add nsw i256` | 631 | 105 | 13 | 66,255 | 8,203 |
| `zext i160→i256` | 553 | 32 | 13 | 17,696 | 7,189 |
| `or disjoint i256` | 543 | 62 | 13 | 33,666 | 7,059 |
| `sub nsw i256` | 480 | 168 | 13 | 80,640 | 6,240 |
| `zext i1→i256` | 475 | 21 | 18 | 9,975 | 8,550 |
| `shl nuw nsw i256` | 447 | 168 | 12 | 75,096 | 5,364 |
| `select i256` | 420 | 55 | 24 | 23,100 | 10,080 |
| `icmp slt i256` | 418 | 73 | 11 | 30,514 | 4,598 |
| `sub i256` | 388 | 168 | 13 | 65,184 | 5,044 |
| `shl i256` | 339 | 168 | 12 | 56,952 | 4,068 |
| `xor i256` | 279 | 62 | 13 | 17,298 | 3,627 |
| `udiv i256` | 109 | **1,150** | 13 | 125,350 | 1,417 |
| `mul nuw nsw i256` | 1 | **458** | 13 | 458 | 13 |
| **TOTAL (measurable)** | | | | **3,503,767** | **817,468** |

Full output in `instr-table.txt`; 111 of the 130 triples are measurable, the rest being control flow,
`phi`, `alloca` and pointer casts that cannot be isolated this way.

**How it is measured.** Marginal cost: the `.text` slope between four and twelve independent
instances of the instruction, with every result consumed through a volatile store so nothing folds
away. An earlier attempt compared against a control function and was biased -- `icmp` measured as
**zero** because the control (`trunc`) costs exactly what the instruction costs with the extension,
and non-volatile loads were deleted as dead. These numbers are also higher than isolated
single-instruction figures (`add i256` is 105 here against 56 measured in isolation) because twelve
live wide values create real register pressure, so the marginal cost includes its share of spilling.
That is the more representative figure for code that exists.

**What it shows.** The two `ult i256` variants alone were 834,025 bytes at baseline -- roughly a
quarter of all measurable code -- and are now 125,675. Division is the extreme per-instruction case
at 1,150 bytes, but only 109 sites.

What remains is no longer dominated by wide arithmetic. The largest surviving entries are narrow
work the extension does not touch (`add nuw i32` 49,423, `load i32` 38,250, `trunc i256→i32`
55,992), and two wide comparisons. `icmp eq i256` at 19 bytes against `ult` at 11 is the one
anomaly worth chasing: equality should be the cheaper of the two, and is not.

---

## 5. What is implemented

`i256` is a legal type with its own register class, so type legalization never splits it.

**Registers.** `i256` is a type `VRM2` holds, so a wide value is an LMUL=2 vector register pair --
exactly 256 bits at VLEN=128. Copies, spills and reloads reuse RVV's own `VMV2R_V`, `VS2R_V` and
`VL2RE8_V`, and arguments use the existing `ArgVRM2s`. An earlier revision used a parallel register
file for this; see the future-improvement note at the end of §2. A decoder reading the ELF sees vector
register numbers. `w4`–`w11` (encoding `v8`–`v22`) are the argument registers, mirroring `ArgVRM2s`.

**Instructions**, all in the custom-2 opcode space, `funct3` selecting operand shape:

| group | instructions |
|---|---|
| arithmetic | `wadd` `wsub` `wmul` `wand` `wor` `wxor` `wdivu` `wdiv` `wremu` `wrem` |
| shifts | `wsll` `wsrl` `wsra` (amount in a GPR) |
| compare | `wseq` `wsne` `wsltu` `wslt` (result in a GPR) |
| move/convert | `wmv` `wtrunc` `wzext` `wsext` `wbswap` |
| memory | `wld` `wst` |
| EVM-specific | `waddmod` `wmulmod` `wexp` `wsignextend` |

`ugt`/`sgt` need no encoding — they are the same instruction with operands swapped. `le`/`ge` forms
are reached by inverting, via `setCondCodeAction(..., Expand)`.

**Calling convention.** Wide values pass in wide registers in both `CC_RISCV` and `CC_RISCV_FastCC`
— the latter matters because revive gives its internal functions `fastcc`, so that is the path most
`i256` arguments actually take. A three-argument wide call now marshals nothing at all:

```
f_call:  addi sp, sp, -8 ; sd ra, 0(sp) ; call opaque ; ld ra, 0(sp) ; addi sp, sp, 8 ; ret
```

against 144 bytes of address arithmetic and copies before. Single operations collapse accordingly:
`add i256` from 28 instructions to `revive.wadd w4, w4, w5`, and `udiv i256` from 1,040 bytes to one
instruction.

**Supporting work**: a `Select_VRM2` pseudo for wide selects (2,036 `phi i256` in the corpus),
constant materialization -- XLen-sized values built in a GPR and widened through target nodes,
anything genuinely wider from the constant pool -- hand-written lowering for 128-bit loads and
stores, and reserving the frame pointer up front so RVV's late stack objects cannot invalidate the
`hasFP` decision.

---

### Tests

`llvm/test/CodeGen/RISCV/xrevivevec-*.ll` -- nine files, 543 checks, generated with
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
| `vlen` | spills are fixed 32-byte slots at immediate offsets, with no `vlenb` arithmetic |

`llvm/test/MC/RISCV/xrevivevec-disasm.txt` covers the decoder for 27 encodings byte-exactly, and
asserts each is invalid without the extension. `check-llvm-codegen-riscv` passes 2454/2454 and the
RVV suite 1200/1200.

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
   prototype — a C++ matcher is invisible to TableGen's type inference.

2. **`DAGCombiner` merged adjacent constant stores into a 256-bit store, then re-widened its own
   result forever.** This hung the largest module. Fixed by overriding `canMergeStoresTo`, which is
   the correct answer anyway: a merged wide constant store needs a constant-pool entry and a load,
   where the unmerged form used plain immediate stores.

3. **`ExpandIRInsts` rewrote wide `udiv`/`urem` into libcalls before they reached the selector**,
   because `MaxDivRemBitWidthSupported` was 128. Raised to 256 when the extension is on.

4. **The calling-convention fast path desynchronised `PendingLocs`** for split aggregates, and the
   rejection surfaced as `llvm_unreachable(nullptr)` — an abort with no message at all. Guarded on
   `!ArgFlags.isSplit() && PendingLocs.empty()`.

5. **Wide spill slots requested 32-byte alignment**, which exceeds the 8-byte stack alignment of
   `LP64E`. The register allocator creates those slots *after* `getReservedRegs` has decided whether
   to reserve the frame pointer, so `emitPrologue` then found `hasFP` true with `X8` unreserved and
   asserted. These loads and stores carry no alignment requirement of their own, so the slot
   alignment was dropped to the stack alignment.

6. **Missing patterns surfaced one at a time** as the corpus exercised them: `bswap` (no generic
   expansion exists at this width, and EVM byte-swaps constantly for big-endian calldata),
   extending loads from `i8`/`i16`/`i32`/`i64`, and truncating stores. `EXTLOAD` cannot be marked
   `Expand` — LegalizeDAG asserts it is always supported — so it needs real patterns. The missing
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

**Register allocation quality: about 117,000 bytes.** The gap to a register file of the extension's
own is now **133,196 bytes**, down from 413,712. Two of its three known components are fixed -- the
scalable spill addressing and the frame pointer, in §2 step 0 -- and what is measurable in the rest
is small:

| | bytes | share of gap |
|---|--:|--:|
| `vsetvli` around vector operations | 9,200 | 7% |
| unattributed | ~124,000 | 93% |

The unattributed part looks like allocation rather than a missing transform. Whole-register spills
number 34,626 across the corpus, and `VRM2` overlaps `VR`, `VRM4` and `VRM8`, so the allocator's
interference model is far more constrained than a standalone 16-register class would be; `v0` also
doubles as the RVV mask register. That is a hypothesis, not a measurement -- confirming it needs the
spill count from a non-overlapping class of the same size.

**Narrowing wide operations whose result only needs XLen bits.** EVM contracts hold `uint32`,
`uint64` and `address` values inside 256-bit words, and once `i256` is a machine type every one is
computed at full width. The limb expansion got the demotion for free: masking against `0xFFFFFFFF`
folded the upper three limbs to literal zeros and the knowledge propagated downstream.

The obvious-looking fix -- `computeKnownBitsForTargetNode` -- was implemented and measured and does
**not** work; see §2, step 3. Known bits were never the missing ingredient. What is needed is a
*narrowing* transform: when the demanded bits of a wide operation fit an XLen register, replace it
and its operands with the scalar instruction. That is where the residual Sha256 regression lives.

---

## 8. What is not done

**The EVM-specific instructions work, but nothing emits them yet.** `waddmod`, `wmulmod`, `wexp` and
`wsignextend` have encodings, intrinsics and patterns, and each selects to a single instruction —
verified:

```
revive.waddmod w4, w4, w5, w6      revive.wexp        w4, w4, w5
revive.wmulmod w4, w4, w5, w6      revive.wsignextend w4, w4, w5
```

What is missing is the resolc side: it still calls the hand-written routines in `stdlib.ll`, so the
measurement above **excludes** this win entirely. The corpus has 57 `__mulmod`, 22 `__addmod` and
18 `__exp` call sites; `__mulmod` is 4,534 bytes and drags in `__ulongrem` at 5,710 for the slow
path that every modulus near 2²⁵⁶ takes, and the production routines plus helpers come to 15,688
bytes. Note this win cannot be measured through the `llc` harness at all — the saving comes from the
routine bodies becoming unreferenced, and only the PVM linker garbage-collects them.

Getting there needed one piece of LLVM plumbing worth calling out: **intrinsics could not take a
256-bit scalar**. The type encoding LLVM uses for intrinsic signatures stopped at `IIT_I128`,
because no in-tree target has a scalar that wide, so the declarations verified as "incorrect return
type". Adding `IIT_I256` and its decoder case fixed it.

**PVM cannot consume the output**, which is the agreed boundary for this phase. `resolc` now runs
end-to-end against this LLVM — `VM_FEATURES` requests `+xrevivevec`, and Solidity → Yul → LLVM IR →
RISC-V object all succeed — and then the polkavm linker rejects the encodings:

```
polkavm linker failed: unsupported instruction in <section #0+726> ('.text') at address 0x2d6: 0x0005445b
```

`0x…5b` is custom-2. Everything up to that point works, and the emitted object is real: ERC20's
`.text` disassembles to 387 `wld`, 180 `wst`, 52 `wtrunc`, 47 `wsrl`, 45 `wsltu`, 41 `wseq`,
37 `wadd`, 32 `wbswap`. A consequence worth stating plainly: on this branch `resolc` cannot produce
a blob for *any* contract, so the branch is an experiment, not something to merge as-is.

What has been demonstrated through `resolc` itself is ERC20 (complete object) and
AutomataDcapAttestationFee (all 16 modules emitted) — the rest of the corpus is covered at the
codegen level by the 103-module measurement above, which is the same compiler and the same IR, but
driven through `llc` rather than the `resolc` front end.

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

**Only 16 wide registers, all caller-saved.** None are in any callee-saved list, so every wide value
live across a call is spilled. Giving some of them callee-saved status is likely worth measuring.

**Compile time.** Two pathologies were found and fixed -- the store-merging loop and the missing
`extloadi64` pattern -- and every module now compiles inside the harness's 120-second limit, where
the largest previously did not. Whole-contract `resolc` runs are still slow: `AutomataDcapAttestationFee`
ran 12 CPU-minutes past codegen without completing its link when this was last measured, before the
constant fix. Worth re-checking and profiling before this goes near CI.

**Nothing verifies semantics.** Stopping before PVM means no differential testing against EVM, so a
wrong lowering is invisible. Every result here is a size measurement, not a correctness one. The
mitigation worth building is a mode that lowers the wide instructions back to scalar limbs, so the
same IR can be differentially tested.

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
