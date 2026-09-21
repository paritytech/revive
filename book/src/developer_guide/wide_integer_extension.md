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

## One extension

`XReviveVec` is what revive's fork of LLVM adds: `i256` becomes a machine type held in a
vector register pair, each wide operation selects to a single instruction in the custom-2
opcode space, and a calling convention of its own passes wide arguments in registers rather
than by reference.

The extension implies nothing. The registers and their pairing are the vector
specification's, because borrowing an already-specified register file beats inventing one,
but no vector type becomes legal on the extension's account and the compiler cannot select
a single standard vector instruction. Everything the code generator emits for wide values
is the extension's own: its arithmetic, its comparisons, its loads and stores, its spills
(`revive.wst`/`revive.wld`) and its copies (`revive.wmv`).

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

## The register width is the extension's own

The vector extensions leave the register width to the implementation and generated code
asks for it at run time, which is why generic vector frames read `vlenb` and grow by
quantities the compiler does not know. `XReviveVec` has none of that: the extension is
defined for registers of exactly 128 bits, the backend states that number itself rather
than taking it from a `Zvl` feature, and every spill slot is a plain 32-byte stack object
at a constant offset.

## In PolkaVM

The register file, `vtype` and `vl` are part of the `ReviveV1` instruction set, alongside
the wide instructions. PolkaVM also implements **a subset of `Zve64x`**, and it is exactly
that: a subset, not the extension. What is in it: the configuration instructions, whole
register moves, loads and stores, unit-stride element loads and stores, the element-wise
integer arithmetic, shifts, comparisons, minimum and maximum, multiply, divide and
multiply-accumulate in all three operand shapes, the splats and the scalar moves, the mask
logic, `vcpop.m`, `vfirst.m`, `vid.v`, `vmerge` and the slides. What is not: the strided
and indexed memory forms, the widening and narrowing operations, the reductions, the
permutes, and masking on anything other than `vcpop.m` and `vfirst.m`. An instruction
outside the subset is refused at link time rather than at run time.

Two consequences of it being a subset. resolc uses none of it: since the extension stopped
implying the vector extensions, the compiler cannot emit a standard vector instruction at
all, and measurements showed the subset was contributing nothing to code size anyway. And
code built by a stock toolchain for standard `Zve64x` will generally fail to link, because
generic code generation freely uses the families the subset omits; building vectors from
scalars alone goes through `vslide1down.vx`. The subset stays implemented and tested in the
interpreter and the recompiler, but nothing targets it today, and a toolchain cannot be
told to target it, because feature flags describe whole extensions rather than subsets.

The wide instructions' semantics are the EVM's rather than Rust's: division and remainder by
zero produce zero, shift amounts of 256 or more clear the value, and `addmod`/`mulmod` keep
the untruncated intermediate.

The interpreter and the recompiler both execute everything above. In the interpreter every
operation on the register file lives in `polkavm-common`. The recompiler maps the register
file onto the host's: wide register `n` is `ymm<n>` for the whole time the guest runs, so
the sixteen wide registers are the sixteen vector registers an x86-64 host has, and every
common wide instruction is a fixed sequence of a few AVX2 instructions chosen at translation
time with no register allocation. A move is one instruction, a load or a store is one
unaligned 256-bit access, the bitwise operations are one instruction each, an equality test
is an exclusive or and a `vptest`, an addition stages its operands below the stack pointer
and runs four carry-linked scalar additions, the constant shifts move whole words between
lanes with the cross-lane permutes and finish with per-word shifts, and the bit counts run
`lzcnt`/`tzcnt` over the four words. A template that needs one more vector register than
the instruction names borrows one, spilled to the red zone for the length of the template.
Multiplication, division, the modular operations, exponentiation, the shifts by a register
amount and the vector subset still go through the shared implementation, by a native helper
that receives the instruction's operands packed at translation time.

The file has to leave the vector registers whenever control leaves the guest, because host
code may use them freely: every exit path writes the sixteen registers into the context,
every entry reads them back, and the helper trampoline does both around its call. An exit
by signal, which is how running out of gas and a page fault arrive, finds them only in the
frame the kernel built, and the signal handlers read them out of its `XSAVE` area. A
recompiled memory access of the vector subset is answered rather than performed by the
helper, with a source, a destination and a length; the bytes move in recompiled code, so
that a page fault lands where the signal handler can attribute it to the guest address the
call site recorded. The execution tests run each backend and, in the tracing configuration,
run both in lockstep; further tests cover every template under every way its operands can
coincide, every word boundary of the constant shifts, and the register file surviving a
host call and an out-of-gas interruption.

Over the openzeppelin contracts this arrangement compiles the wide instructions to 55% less
native code than the helper-per-instruction one it replaces, 479,142 bytes against 213,234
with the scalar code unchanged, and the whole recompiled contracts are 39% smaller. A wide
move went from 36 bytes of native code to 4.5, a load or store from 41 to 11, a bitwise
operation from 54 to 4, an equality test from 57 to 27, a sign extended immediate from 44 to
9. An addition grew from 44 to 92, because it is now straight-line code that assembles its
result words directly into the registers rather than a call, and a constant shift from 26
to 37 for the same reason. In a loop of the instructions contracts consist of, the recompiled code runs 2.5 times
faster than under the helper arrangement: moves and bitwise operations ten times, loads and
stores twice, constant shifts four times, a chain of dependent additions 17% slower,
comparisons about the same, and an operation that still goes through the helper about 7 ns
slower for the larger save. The recompiler requires AVX2 for these
instructions, as it already required BMI2 for everything else.

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
become 148,784. The custom instructions carry the entire reduction: compiling with every
form of vectorization disabled produces byte-identical blobs, and enabling the standard
vector extension without the custom instructions makes every contract slightly larger than
the baseline.

| contract | without | with | delta |
|---|--:|--:|--:|
| erc1155 | 30,649 | 16,243 | -47.0% |
| erc20 | 42,893 | 21,903 | -48.9% |
| erc721 | 49,423 | 23,966 | -51.5% |
| oz_gov | 81,159 | 40,128 | -50.6% |
| oz_rwa | 37,975 | 18,290 | -51.8% |
| oz_simple_erc20 | 16,554 | 7,806 | -52.8% |
| oz_stable | 39,032 | 17,721 | -54.6% |
| proxy | 3,577 | 2,727 | -23.8% |
| **total** | **301,262** | **148,784** | **-50.6%** |

The Phase 1 report measured an earlier arrangement of the same idea and answered the width
and addressing questions on it: `Zvl128b` against `Zvl256b` was 44 bytes, and pinning the
vector length made no difference at all, leaving the spill slots scalable and the `csrr
vlenb` sequences in place. The second of those no longer holds: pinning the length is now
what removes them, because the backend was taught to act on it. It is in
[Measurements](./wide_integer_analysis.md).
