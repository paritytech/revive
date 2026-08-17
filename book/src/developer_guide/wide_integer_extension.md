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

The registers are the extension's own rather than an alias of the vector file. That keeps
the extension free of any dependency on the vector extensions: no vector type becomes legal,
so nothing else in the backend can select a vector instruction, and every operation on a
wide value is one custom-2 encoding. Copies are `revive.wmv`, and spills are
`revive.wst`/`revive.wld` against ordinary fixed-size stack slots, so a spill costs one
instruction rather than a vector-length query and scalable frame arithmetic.

## In PolkaVM

The register file and the instructions are part of the `ReviveV1` instruction set. Their
semantics are the EVM's rather than Rust's: division and remainder by zero produce zero,
shift amounts of 256 or more clear the value, and `addmod`/`mulmod` keep the untruncated
intermediate. The interpreter implements all of them; the recompiler does not, and refuses
to compile a module that uses them rather than producing one that traps.

## Status

Experimental, and behind the `+xrevivevec` target feature.

Over the openzeppelin contracts in `oz-tests`, PVM blobs are **45% smaller**. The full
analysis, per-benchmark results, the VLEN and VLA/VLS comparisons, what the extension does
not cover, and the work still on the table, is in
[Measurements](./wide_integer_analysis.md).
