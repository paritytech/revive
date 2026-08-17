# The wide integer extension (XReviveVec)

## The problem

EVM words are 256 bits. RISC-V registers are 64. Every EVM operation therefore becomes a
chain of four `i64` limbs, and because RISC-V has no carry flag each carry has to be
materialised in a register: an `sltu` to produce it and another to consume it.

That expansion dominates revive's code size. Across the 15 benchmark contracts, `i256`
accounts for a quarter of all LLVM IR reaching the backend, and one `add i256` costs 56
bytes where the scalar add costs 2. `icmp ult i256` costs 86, because the expansion walks
limbs from the top and branches at each one.

The calling convention makes it worse. Anything wider than two registers is passed *by
reference* and returned through a hidden pointer, so a call taking three 256-bit arguments
spends 192 bytes of stack frame and 56 instructions marshalling values before the callee
starts.

## Two layers

The registers wide enough to hold an EVM word are the ones the vector extensions specify, so
this is built in two parts that can be used separately.

The lower one is the vector extensions themselves: PolkaVM implements `Zve64x` at a vector
length of 128 bits, so ordinary vector code runs on it and nothing about it is specific to
revive. A source-level toolchain reaches it with `-march=rv64em_zve64x_zvl128b` and
`-mrvv-vector-bits=zvl`, and needs nothing else.

The upper one is `XReviveVec`, which is what revive's fork of LLVM adds: `i256` becomes a
machine type held in a vector register pair, each wide operation selects to a single
instruction in the custom-2 opcode space, and a calling convention of its own passes wide
arguments in registers rather than by reference.

```text
add i256      30 instructions  ->  revive.wadd v8, v8, v10
icmp ult      22 instructions  ->  revive.wsltu a0, v8, v10
3-arg call    56 instructions  ->  9 instructions, no marshalling
              192-byte frame        16-byte frame
```

## The register file

There is one register file, seen two ways. A vector register holds 128 bits; a wide register
is the pair `v2n`, `v2n+1`, which is what a register group of two is at that length. So
`vmv2r.v v8, v12` and the wide move of the same values are the same instruction, and the
linker emits the shorter spelling for the aligned pairs that generated code actually uses.

The wide instructions name a pair in four bits, which is why a three-operand one is three
bytes rather than four; a general vector instruction names a register in a byte.

## The calling convention

Wide arguments and returns travel in `v8` to `v23`, eight pairs, which is where the standard
vector calling convention puts vector arguments. The one departure is that `v0` to `v7` and
`v24` to `v31` are callee saved, where the standard convention has no callee-saved vector
register at all: wide values are what revive keeps live across calls, and without this the
caller spilled and reloaded every one of them at every call site.

## Fixing the vector length

The vector extensions leave the register width to the implementation, and generated code
asks for it at run time: `vlenb` is read to size a spill slot, and the frame grows by a
multiple of a quantity the compiler does not know. PolkaVM's length is architectural rather
than an implementation choice, and `XReviveVec` is defined for a machine that has it, so the
backend takes the exact value from the extension and all of that collapses to constants. It also leaves `vl` and `vtype` known at every vector instruction,
which is what an ahead-of-time recompiler needs in order to translate them.

Three things in the backend follow from a known length, and are what makes the difference
between a frame with a vector region in it and an ordinary one: a vector spill slot is a
plain stack object rather than one whose offset is scaled at run time, its size is the
register class's size scaled up to the real width, and the whole-frame alignment that the
vector region would demand is not imposed.

## In PolkaVM

The register file, `vtype` and `vl` are part of the `ReviveV1` instruction set, alongside
the wide instructions. What is implemented of the vector extensions is: the configuration
instructions, whole register moves, loads and stores, unit-stride element loads and stores,
the element-wise integer arithmetic, shifts, comparisons, minimum and maximum, multiply,
divide and multiply-accumulate in all three operand shapes, the splats and the scalar moves,
the mask logic, `vcpop.m`, `vfirst.m`, `vid.v`, `vmerge` and the slides. What is not: the
strided and indexed memory forms, the widening and narrowing operations, the reductions, the
permutes, and masking on anything other than `vcpop.m` and `vfirst.m`. An instruction outside
that set is refused at link time rather than at run time.

The wide instructions' semantics are the EVM's rather than Rust's: division and remainder by
zero produce zero, shift amounts of 256 or more clear the value, and `addmod`/`mulmod` keep
the untruncated intermediate.

The interpreter and the recompiler both execute everything above, out of one implementation:
every operation on the register file lives in `polkavm-common`, the interpreter calls it
directly, and recompiled code reaches it through a native helper that receives the
instruction's operands packed at translation time. A recompiled memory access is answered
rather than performed by the helper, with a source, a destination and a length; the bytes
move in recompiled code, so that a page fault lands where the signal handler can attribute
it to the guest address the call site recorded. The execution tests run each backend and,
in the tracing configuration, run both in lockstep.

## Encoding

Wide operands are nibbles, two to a byte, so a three-operand instruction is three bytes. A
destination that repeats the first source is left out, which the register allocator arranges
for well over half of them, and those instructions are two bytes instead.

A value that was only ever loaded into a general purpose register to feed a wide instruction
does not need the register at all. The linker folds it into the instruction, which gives the
widening, the shifts and the load from a fixed address immediate forms of their own.

The element-wise vector instructions go the other way. They are one opcode carrying the
operation and its three operands in a single immediate, because there are enough of them
that an opcode each would not fit in the byte the instruction set has, and they are rare
enough in practice that the extra byte does not matter.

## Status

Experimental, and behind the `+xrevivevec` target feature, which resolc requests by
default. Setting `RESOLC_DISABLE_WIDE_INTEGERS` in the environment compiles without the
extension, which is how the baseline column below is produced, and what a bisection or a
blob for a PolkaVM without the instructions uses.

Over the openzeppelin contracts in `oz-tests`, PVM blobs are **50% smaller**: 301,262 bytes
become 148,847.

| contract | without | with | delta |
|---|--:|--:|--:|
| erc1155 | 30,649 | 16,243 | -47.0% |
| erc20 | 42,893 | 21,907 | -48.9% |
| erc721 | 49,423 | 23,981 | -51.5% |
| oz_gov | 81,159 | 40,147 | -50.5% |
| oz_rwa | 37,975 | 18,303 | -51.8% |
| oz_simple_erc20 | 16,554 | 7,806 | -52.8% |
| oz_stable | 39,032 | 17,733 | -54.6% |
| proxy | 3,577 | 2,727 | -23.8% |
| **total** | **301,262** | **148,847** | **-50.6%** |

The Phase 1 report measured an earlier arrangement of the same idea and answered the width
and addressing questions on it: `Zvl128b` against `Zvl256b` was 44 bytes, and pinning the
vector length made no difference at all, leaving the spill slots scalable and the `csrr
vlenb` sequences in place. The second of those no longer holds: pinning the length is now
what removes them, because the backend was taught to act on it. It is in
[Measurements](./wide_integer_analysis.md).
