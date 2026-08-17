# Support for Vector extension and custom instructions in Revive

# Overview

Due to high pressure outside to get better code size need to enable
RISC-V vector extension in revive + llvm and add custom instructions
for 128, 256 (maybe to 512) bit integers for operations that are widely used
in contract

# Details

The idea is to enable Zvl128 + VLS and introduce custom instructions RISC-V instructions
that would help to decrease code size and pass high-level instructions to PVM, which
will later interpret/recompile them correctly.
**PVM work is not in the scope for this task**
It's crucial first to familizer yourself first with current analysis
`/Users/nikolaypanchenko/workspaces/revive/benchmarks/analysis/carry-ext/RECAP.md`

After that, for all wide arithmetic operations, comparisons and functions from `stdlib.ll`,
that are used by contracts, add single special RISC-V instruction that helps to avoid type
legalization done by LLVM.
For operations with operands >= 128bit, operands are passed using vector registers and
operation itself, despite vector operands, does single scalar operation.

For example,
```
%0 = add i256 %x, %y
```
introduce `swadd <vreg> <vreg>, <vreg>`:
```
vsetivli 4, m2, e64
vle64 v0, %x # load i256 into v0, v1 (because of LMUL=2)
vle64 v0, %x # load i256 into v2, v3 (because of LMUL=2)
swadd v4, v0, v2 
vse64 v4, %0 # store i256from v4 into %0
```

ABI should also be updated to pass wide values using vectors, i.e.
```
define i256 @foo(i256 %arg0) {
    %ret = ...
    ret i256 %ret
}

...

%0 = call @foo(%x)
```

should look like
```
.function foo
  # v8, v9 contain data of %arg0
  
  ret # v8, v9 or some other according to default RISC-V vector CC
```


# Phase 1
From analysis doc, implement custom instructions for all arithemtic, compare and `stdlib.ll`. Enable `Zvl128` + VLS in revive.
You're fine to consider that PVM is not ready, therefore you can stop validation before giving ELF to PVM
To do revive changes, use `cl/custom-ops` in this repo; LLVM changes go to the llvm-project fork's `cl/custom-ops` branch, which the submodule tracks.
The goal:
- all revivew tests, all benchmarks from the `RECAP.md` doc can be compiled.
- LLVM never legalized wide type for instructions of interest and always generates custom instructions
- Wide function arguments are now passed via vector register

At the end of this phase it's expected you build a comprehensive report: what's implemented, what can be improved, what went or could go wrong with existing approach (ignore portability to other HWs). Per-benchmark breakdown of codesize change with and without your changes

# Phase 2
Using these benchmarks, compare
- Zvl128 vs Zvl256
- Zvl128 VLA vs Zvl128 VLS

# Phase 3
Dump which other wide LLVM instructions from these contracts are not yet covered by custom instructions
