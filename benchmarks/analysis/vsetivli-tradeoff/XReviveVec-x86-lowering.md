# XReviveVec — recompiler x86-64 lowering per wide instruction

What native x86-64 the PolkaVM **recompiler** emits for each of the 32 XReviveVec wide instructions. Two paths: **inline** native code (the cheap/common ops, the default) and the **`syscall_wide` trampoline** (heavy ops, and any inline op at i512+). Source of truth: `polkavm/crates/polkavm/src/compiler/amd64.rs` (`wide_*` methods); inline is gated by `POLKAVM_DISABLE_WIDE_INLINE` (unset = inline).

### Conventions

- **`rcx`** — `TMP_REG`, the recompiler's single general scratch (nothing else is free, so every inline form uses only this).
- **`[Wr.i]`** — 64-bit limb `i` of wide register `r` in the VmCtx wide file, i.e. `[vmctx + wide_off + (r.limb_offset + i)*8]`. All limb accesses carry a 4-byte `disp32`.
- **`gpr(x)`** — the native register a guest GPR `x` maps to (`conv_reg`); never `rcx`.
- **`<sc>`** — `WIDE_SCRATCH_REG`, a callee-preserved native register `push`/`pop`ed around a wide load/store.
- **Limbs:** i128 = 2, i256 = 4, i512 = 8, i1024 = 16. Inline forms are gated to **≤ i256** (limbs ≤ 4) unless noted; wider falls back to the trampoline.
- The **96-byte per-instruction native-code budget** (`VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH`) is why the inline forms stop at i256.

---

## Inline forms (default)

### `add` `sub` `and` `or` `xor` — `wide_binop_inline`

Two shapes, chosen by operand aliasing. `add`/`sub` thread the carry/borrow through `adc`/`sbb`; the intervening `mov` does not disturb the flags. `and`/`or`/`xor` have no carry (same op every limb).

**In-place** — when the destination aliases a source (commutative ops may pick either source). 2 instr/limb, fits any width:

```asm
; d <op>= other   (per limb i = 0 .. N-1)
mov   rcx, [Wother.i]
add   [Wd.i], rcx        ; i == 0            (sub→sub, and→and, or→or, xor→xor)
adc   [Wd.i], rcx        ; i  > 0, add/sub only (sub→sbb)
```

**Three-address** — distinct `d`, `s1`, `s2`. 3 instr/limb; i256 = 88 B (≤ 96 B cap), **i512+ → trampoline**:

```asm
; d = s1 <op> s2   (per limb i)
mov   rcx, [Ws1.i]
add   rcx, [Ws2.i]       ; i == 0            (adc for i>0 on add; sub/sbb; and/or/xor each limb)
mov   [Wd.i], rcx
```

### `move` — `wide_move`

Limb copy; emits **nothing** when `d` aliases `s1`. ≤ i256.

```asm
; per limb i
mov   rcx, [Ws1.i]
mov   [Wd.i], rcx
```

### `trunc` — `wide_truncate` (any width)

The result is `s1`'s low limb — a single load into the destination GPR:

```asm
mov   gpr(d), [Ws1.0]
```

### `zext` / `widen_unsigned` — `wide_widen_unsigned` (≤ i256)

Scalar into limb 0, zero the rest:

```asm
mov   [Wd.0], gpr(s1)
xor   ecx, ecx
mov   [Wd.i], rcx        ; i = 1 .. N-1
```

### `slt_u` / `slt_s` — `wide_slt_inline` (≤ i256, ~63 B)

A subtract-with-borrow chain over the file; the final flag is the answer (CF = unsigned borrow; SF≠OF on the top limb = signed less-than). The result GPR is pre-zeroed so `setcc` on its low byte yields a clean 0/1 with no `movzx`; the `mov` loads between the `sbb`s preserve CF.

```asm
xor   gpr(d), gpr(d)     ; 32-bit, pre-zero result
mov   rcx, [Ws1.0]
sub   rcx, [Ws2.0]
mov   rcx, [Ws1.1]       ; mov preserves CF
sbb   rcx, [Ws2.1]
; ... one mov + sbb per remaining limb ...
setb  gpr(d)             ; slt_u (Below);  slt_s → setl (Less)
```

### `seq` / `sne` — `wide_equal_inline` (≤ i256, ~74 B)

XOR-OR fold, then `setcc` on ZF. The accumulator lives in the result GPR, so a flag-preserving `mov d,0` precedes `setcc`:

```asm
mov   gpr(d), [Ws1.0]
xor   gpr(d), [Ws2.0]
mov   rcx, [Ws1.i]       ; i = 1 .. N-1
xor   rcx, [Ws2.i]
or    gpr(d), rcx
mov   gpr(d), 0          ; preserves ZF from the final OR
sete  gpr(d)             ; seq (Equal);  sne → setne
```

### `load` / `store` — `wide_access`

The guest address is materialised **once** with `lea` (to fit the byte budget); limbs then index off it. ≤ i256 is unrolled:

```asm
push  <sc>
lea   rcx, [guest_base + offset]
; per limb i (load shown; store swaps the two mov directions):
mov   <sc>, [rcx + i*8]
mov   [Wd.i], <sc>
; ...
pop   <sc>
```

Wider than i256 walks a counted loop whose size is width-independent (two pointers + counter; **64-bit** pointer adds — a 32-bit `add` here truncated the >4 GiB wide-file pointer and faulted, now fixed):

```asm
push  <sc> ; push <cnt> ; push <fp>
lea   rcx, [guest_base + offset]
lea   <fp>, [Wr.0]
mov   <cnt>, N
body:
  mov   <sc>, [rcx]          ; (store: mov <sc>, [<fp>])
  mov   [<fp>], <sc>         ; (store: mov [rcx], <sc>)
  add   rcx, 8               ; imm64 (REX.W) — 64-bit pointer arithmetic
  add   <fp>, 8
  sub   <cnt>, 1
  jne   body
pop   <fp> ; pop <cnt> ; pop <sc>
```

---

## Trampoline forms

**Ops:** `mul`, `div_u`/`div_s`, `rem_u`/`rem_s`, `exp`, `add_mod`, `mul_mod`, `signext`, `sext_w`, `min_u`/`min_s`, `max_u`/`max_s`, `bswap`, `shl`/`shr_l`/`shr_a` — plus any of the inline ops at i512+. Heavy ops are trampolined by design (µs of real work dwarf the crossing); the shift/`min`/`max`/`bswap`/`sext` group awaits assembler shift and flag-preserving-loop primitives.

**Call site** — `wide_op`. A packed 64-bit descriptor (op · width · d · s1 · s2 · s3) goes in `rcx`; a scalar argument (shifts, `zext`, `sext`) is staged in the VmCtx first; a result (compares, `trunc`) comes back in `rcx`:

```asm
mov   [vmctx.wide_scalar], gpr(scalar)   ; only for shift / zext / sext
mov   rcx, <descriptor imm64>
call  wide_trampoline
mov   gpr(result), rcx                    ; only for compares / trunc
```

**Trampoline body** — `emit_wide_trampoline`, emitted **once** per module (so it costs nothing per instruction). `syscall_wide` is a pure helper over the wide file — it never touches the guest GPRs or resumes the guest — so only caller-saved-mapped registers round-trip the VmCtx:

```asm
wide_trampoline:
  push  rcx
  ; save caller-saved guest regs -> vmctx
  mov   rcx, <&syscall_wide>
  pop   rdi                 ; descriptor -> arg0 (System V)
  call  rcx
  push  rax                 ; result
  ; restore caller-saved guest regs <- vmctx
  pop   rcx                 ; result -> rcx
  ret
```

---

## Summary

| wide op | path | x86 shape | native bytes @256 |
|---|---|---|--:|
| `add` `sub` | inline | `mov`+`adc`/`sbb` chain (in-place 2/limb, else 3/limb) | 56 / 88 |
| `and` `or` `xor` | inline | `mov`+`<op>` chain | 56 / 88 |
| `move` | inline | `mov`/`mov` per limb | 56 |
| `trunc` | inline | single `mov` (low limb) | 7 |
| `zext` | inline | store limb 0 + `xor`+zero-fill | 30 |
| `slt_u` `slt_s` | inline | `sub`/`sbb` chain → `setb`/`setl` | ~63 |
| `seq` `sne` | inline | XOR-OR fold → `sete`/`setne` | ~74 |
| `load` `store` | inline | `lea` + per-limb `mov` (loop > i256) | 52 |
| `mul` | trampoline | descriptor + `call` | 10 (call site) |
| `div_u/s` `rem_u/s` `exp` `add_mod` `mul_mod` | trampoline | descriptor + `call` | 10–15 |
| `shl` `shr_l` `shr_a` | trampoline | scalar store + descriptor + `call` | 17 |
| `sext_w` `signext` | trampoline | (scalar store +) descriptor + `call` | 10–17 |
| `min_u/s` `max_u/s` `bswap` | trampoline | descriptor + `call` | 10 |
| any inline op at **i512+** | trampoline | descriptor + `call` | 10–17 |

The inline forms are what bring the recompiler to **1.00× the base ISA** whole-contract (the trampoline path is ~23% slower); see the ns/op and gas tables in the main reference. Byte counts are the per-instruction native-code emission (the shared trampoline body is emitted once and is off-budget).
