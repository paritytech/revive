# How XReviveVec should carry a wide instruction's width

Four ways of answering "how wide is this `revive.wadd`?", measured end to end: LLVM IR in, PolkaVM
blob out, run on the interpreter.

| arm | width comes from | LLVM | linker |
|---|---|---|---|
| `ref` | — (no extension) | — | — |
| `no-vsetvli` | `funct7[6:5]` of the instruction | per-width mnemonics, one custom-2 space | reads it off the instruction |
| `vsetvli-noMO` | `vtype`, set by `vsetivli` | one mnemonic per operation; outliner barred from lifting vtype-dependent code | CFG dataflow |
| `vsetvli-MO` | `vtype`, set by `vsetivli` | outlining allowed | interprocedural dataflow |

Corpus: 103 modules of post-optimization IR at `-Oz` from the 15-contract benchmark set.

## The gas column is not a performance measurement

`runblob` sets no cost model, so PolkaVM falls back to `CostModel::naive()`, in which *every*
instruction costs one gas -- a 256-bit `revive.wmul` included. There is no better option available:
`CostModelKind::Full` routes through `simulator.rs`, which `unimplemented!()`s on every wide
instruction and would panic. Assigning real costs to the 32 wide instructions is still open work.

So "gas" below means **dynamic instruction count**, and against `ref` it flatters the extension
badly: one `revive.wmul` standing in for a ~40-instruction limb chain scores as a 39-instruction
saving when the real work is nothing like 1/40th. **Do not read the gas/ref column as a speed-up.**

Between the three extension arms the figure is more defensible -- they execute the same wide
instructions in the same places and differ mainly in call and outlining overhead -- but it is still
a count, not a time.

Everything ran on the **interpreter**. The recompiler is x86-64 and requires BMI2; the measuring
host is arm64, so it could not run at all. A real performance number needs either gas costs for the
wide instructions or an x86-64 Linux host.

## Results

Code and blob size over the 61 modules every arm compiled, linked and ran. Gas over the 108
exports every arm ran to the same outcome after the same number of host calls — an arm that faults
early burns little gas while doing none of the work, so unmatched exports would read as a speed-up.

| arm | `.text` | blob | gas | text/ref | **blob/ref** | gas/ref |
|---|---|---|---|---|---|---|
| `ref` | 193,400 | 242,022 | 313,765 | — | — | — |
| `no-vsetvli` | 154,642 | 167,905 | 262,162 | −20.04% | −30.62% | **−16.45%** |
| `vsetvli-noMO` | 171,140 | **165,935** | 264,975 | −11.51% | **−31.44%** | −15.55% |
| `vsetvli-MO` | 164,758 | 176,311 | 265,135 | −14.81% | −27.15% | −15.50% |

Compile time, fastest of three runs over the 83 modules `llc` compiled in every arm:

| arm | compile |
|---|---|
| `ref` | 7.3 s |
| `no-vsetvli` | 5.0 s |
| `vsetvli-noMO` | 5.2 s |
| `vsetvli-MO` | 5.4 s |

Link time is 0.9–1.1 s across all arms; the dataflow does not show up in it.

## Every figure above is a sum, and the sums are concentrated

The size and gas columns add up 61 modules; one of them is 24.6% of the total blob bytes and the
top five are 49.8%. Compile time comes from a separate run over 83 modules, so it does not share
the others' basis. Read the totals as "what the corpus costs in total", not as "what a typical
contract costs".

Per module, `vsetvli-noMO` against `no-vsetvli` on blob size, over the 94 modules both linked:

| | |
|---|---|
| sum | −3.20% |
| mean | −0.10% |
| **median** | **+1.58%** |
| smaller on | 33 modules |
| larger on | 61 modules |

So width-from-`vtype` is *not* smaller on a typical contract. It is smaller on big ones — −3.54%,
−4.74% and −6.69% on the three largest — and costs a near-constant ~19 bytes on small ones. The
claim the totals support is that it scales better, not that it always wins.

Dropping the `ref` column widens the comparison from 61 modules to 94, because 33 were excluded
only because `ref` cannot lower the `revive.*` intrinsics at all:

| arm (94 modules) | `.text` | blob | blob vs funct7 |
|---|---|---|---|
| `no-vsetvli` | 1,028,558 | 1,345,724 | — |
| `vsetvli-noMO` | 1,290,866 | 1,302,659 | **−3.20%** |
| `vsetvli-MO` | 1,177,362 | 1,511,771 | **+12.34%** |

## What the numbers say

**The object-level penalty does not survive translation.** At `.text` level `funct7` looks decisively
better: −20.04% against −11.51%, a 10.7-point gap, most of it the outlining that width-in-`vtype`
gives up. At blob level that gap closes and reverses in aggregate: `vsetvli-noMO` totals the
smallest blob of any arm, though see the per-module distribution above before reading that as a
uniform win.
The `vsetivli` instructions carry no PolkaVM instruction and the linker drops them, and PolkaVM's
own optimizer recovers what the machine outliner was doing. Measuring RISC-V `.text` overstates the
cost of `vtype` by an order of magnitude.

**Outlining is not worth having.** `vsetvli-MO` is the largest of the three extension arms at blob
level -- +12.34% against `funct7` over the 94-module set -- and the slowest to compile, and it needs
the most linker machinery. Whatever the outliner
saves in RISC-V is more than lost by the time PolkaVM has finished with it.

**Instruction count barely distinguishes them.** All three land within one point of each other
(−16.45%, −15.55%, −15.50%), which is the one comparison the naive cost model supports. On the
evidence available the width mechanism is a compile-time and link-time question; whether it is also
a performance one is untested.

**Compilation is faster with the extension than without,** by about 30% — i256 as a machine type is
less work than expanding it into limb chains. `vtype` costs 4–8% against `funct7`.

## Correctness

`funct7` cannot get the width wrong: it is in the instruction. `vtype` can, and did.

Two real defects were found and fixed while building this:

- **`vtype` was assumed to survive calls but nothing restored it.** `hasCallPreservedVType()`
  returned true for the extension, so a caller reconfigured nothing after a call, while
  `CalleeSavedRegs<(add ..., VL, VTYPE)>` never took effect because `VTYPE` is a *reserved*
  register and reserved registers are not spilled as callee-saved. A caller at m2 calling a
  function that configured m4 resumed at m4: reloads read 64 bytes where 32 were spilled. Now
  behind `-riscv-revive-call-preserved-vtype`, off by default.
- **Relocated wide accesses did not link.** A `%lo` on `revive.wld`/`revive.wst` had no handler, in
  either design — 27 per module in ERC20 alone. Pre-existing, and fixed in both.

**A residual defect remains.** In 3 of 103 modules (XENCrypto variants) a wide instruction sits
directly after a call with no configuration between it and the call, in the same basic block, so no
other predecessor can supply one:

```
    dc0: jalr    ra                  # call
    dc4: revive.wzext  v8, zero      # runs under whatever the callee left
```

The linker's dataflow reports this rather than guessing, which is how it was found. It is an
LLVM-side bug in the `vtype` design, not a linker limitation, and it is a class of bug `funct7`
does not have.

## Recommendation

`vsetvli-noMO`, on the strength of how it scales: it totals the smallest blob, wins by 3.5-6.7% on
the largest contracts, keeps custom-1 and custom-3 free, and costs about 4% compile time and one
point of instruction count against `funct7`. The recommendation rests on code size alone --
performance was not measured, see above. Against it: it is ~1.58% *larger* on the median contract, so if small
contracts dominate what actually ships, `funct7` is the better choice and this recommendation should
flip. The residual post-call defect must be fixed
first — it is three modules, but it is a wrong-code bug, not a link failure.

`vsetvli-MO` should be dropped: it is bigger, slower to compile, and needs interprocedural analysis
in the linker to buy a regression.

## Reproducing

```
benchmarks/analysis/vsetivli-tradeoff/compare.py     # .text and compile time
benchmarks/analysis/vsetivli-tradeoff/sweep.py       # blob, gas, link time, end to end
```

`bin/` holds the two `llc` builds and the two `polkatool` builds the arms need; `*.patch` are the
working-tree changes each was built from.
