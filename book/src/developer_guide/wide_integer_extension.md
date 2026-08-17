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

## The approach

`XReviveVec` makes `i256` a machine type. It lives in `WREG`, a file of sixteen 256-bit
registers named `w0` to `w15`, and each wide operation selects to a single instruction in
the custom-2 opcode space instead of a limb chain. Wide arguments travel in `w0` to `w7`
rather than by reference.

```text
add i256      30 instructions  ->  revive.wadd w0, w0, w1
icmp ult      22 instructions  ->  revive.wsltu a0, w0, w1
3-arg call    56 instructions  ->  9 instructions, no marshalling
              192-byte frame        16-byte frame
```

## Why not the vector registers

The extension was specified to hold wide values in the vector registers, with `Zvl128b`
enabled. It does not. This is a departure from that design, and the reason is what enabling
the vector extensions brings with it rather than anything about the registers themselves.

Adding `i256` to `VRM2` makes the extension imply `Zve64x` and `Zvl128b`, and once a vector
type is legal the rest of the vector backend follows. A single compiled contract produced
`vsetvli`, `vle8.v` from `memset` lowering, `vmsne.vv` with `vcpop.m` from a 128-bit
equality the DAG combiner vectorised, and `vs2r.v`/`vl2r.v` spills addressed through
`csrr vlenb`. None of that is wide arithmetic, and all of it would have had to be
implemented in PolkaVM: a vector unit with `vtype` state, mask registers and the vector
length CSR, on the recompiler as well as the interpreter.

Giving the extension registers of its own means no vector type is legal, so nothing in the
backend can select a vector instruction on its account, and every operation on a wide value
is one custom-2 encoding. Copies are `revive.wmv`, and spills are
`revive.wst`/`revive.wld` against ordinary fixed-size stack slots, so a spill costs one
instruction rather than a vector-length query and scalable frame arithmetic.

## In PolkaVM

The register file and the instructions are part of the `ReviveV1` instruction set. Their
semantics are the EVM's rather than Rust's: division and remainder by zero produce zero,
shift amounts of 256 or more clear the value, and `addmod`/`mulmod` keep the untruncated
intermediate. The interpreter implements all of them; the recompiler does not, and refuses
to compile a module that uses them rather than producing one that traps.

## Encoding

Wide operands are nibbles, two to a byte, so a three-operand instruction is three bytes. A
destination that repeats the first source is left out, which the register allocator arranges
for well over half of them, and those instructions are two bytes instead.

A value that was only ever loaded into a general purpose register to feed a wide instruction
does not need the register at all. The linker folds it into the instruction, which gives the
widening, the shifts and the load from a fixed address immediate forms of their own.

## Status

Experimental, and behind the `+xrevivevec` target feature.

Over the openzeppelin contracts in `oz-tests`, PVM blobs are **50% smaller**: 301,262 bytes
become 148,713.

| contract | without | with | delta |
|---|--:|--:|--:|
| erc1155 | 30,649 | 16,198 | −47.1% |
| erc20 | 42,893 | 21,885 | −49.0% |
| erc721 | 49,423 | 23,958 | −51.5% |
| oz_gov | 81,159 | 40,128 | −50.6% |
| oz_rwa | 37,975 | 18,290 | −51.8% |
| oz_simple_erc20 | 16,554 | 7,806 | −52.8% |
| oz_stable | 39,032 | 17,721 | −54.6% |
| proxy | 3,577 | 2,727 | −23.8% |
| **total** | **301,262** | **148,713** | **−50.6%** |

The Phase 1 report measured the vector-register design, and answered the width and
addressing questions on it: `Zvl128b` against `Zvl256b` was 44 bytes, and pinning the vector
length made no difference at all, leaving the spill slots scalable and the `csrr vlenb`
sequences in place. Those results do not carry over, since none of the configurations exist
here. It is in [Measurements](./wide_integer_analysis.md).
