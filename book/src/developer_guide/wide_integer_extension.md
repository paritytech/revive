# The wide integer extension (XReviveVec)

## The problem

EVM words are 256 bits. RISC-V registers are 64. Every EVM operation therefore becomes a chain of
four `i64` limbs, and because RISC-V has no carry flag each carry has to be materialised in a
register: an `sltu` to produce it and another to consume it.

That expansion dominates revive's code size. Across the 15 benchmark contracts, `i256` accounts for
a quarter of all LLVM IR reaching the backend, and one `add i256` costs 56 bytes where the scalar
add costs 2. `icmp ult i256` costs 86, because the expansion walks limbs from the top and branches
at each one.

The calling convention makes it worse. Anything wider than two registers is passed *by reference*
and returned through a hidden pointer, so a call taking three 256-bit arguments spends 192 bytes of
stack frame and 56 instructions marshalling values before the callee starts.

## The approach

`XReviveVec` makes `i256` a machine type. It is added to the `VRM2` register class, so at VLEN=128 a
wide value is an LMUL=2 vector register pair, and each wide operation selects to a single
instruction in the custom-2 opcode space instead of a limb chain. Wide arguments travel in those
registers rather than by reference.

```
add i256      30 instructions  ->  revive.wadd v8, v8, v10
icmp ult      22 instructions  ->  revive.wsltu a0, v8, v10
3-arg call    56 instructions  ->  9 instructions, no marshalling
              192-byte frame        16-byte frame
```

The instructions are scalar in meaning: they read neither `vtype` nor `vl`, and their width is fixed
by the opcode rather than by a preceding `vsetvli`. The vector registers are used because they are
the registers wide enough to hold the values, and because the register allocator then knows these
operands overlap ordinary vector values.

## Status

Experimental, and behind the `+xrevivevec` target feature. PVM cannot yet decode the custom-2
encodings, so `resolc` compiles through code generation and then fails at the PolkaVM linker; the
measurements below stop at the object file.

Over the 15 benchmark contracts (103 modules): **−30% code, −26% including the constant pool**, with
every contract improving except `Sha256`.

The full analysis -- per-benchmark results, the VLEN and VLA/VLS comparisons, what the extension
does not cover, and the work still on the table -- is in
[Measurements](./wide_integer_analysis.md).
