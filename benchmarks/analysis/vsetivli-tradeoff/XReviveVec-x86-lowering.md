# XReviveVec — recompiler x86-64 lowering per wide instruction

What native x86-64 the PolkaVM **recompiler** emits for each of the 32 XReviveVec wide instructions. Two paths: **inline** native code (the cheap/common ops, the default) and the **`syscall_wide` trampoline** (heavy ops, and any inline op at i512+). Source of truth: `polkavm/crates/polkavm/src/compiler/amd64.rs` (`wide_*` methods); inline is gated by `POLKAVM_DISABLE_WIDE_INLINE` (unset = inline).

Every inline sequence below is shown **fully expanded at i256 (4 limbs)** — the shipped 99.9% width — with no elision. i128 drops limbs 2–3; i512/i1024 exceed the 96-byte budget and take the trampoline.

### Conventions

- **`rcx`** — `TMP_REG`, the recompiler's single general scratch (nothing else is free, so every inline form uses only this).
- **`[Wr.i]`** — 64-bit limb `i` of wide register `r` in the VmCtx wide file, i.e. `[vmctx + wide_off + (r.limb_offset + i)*8]`. All limb accesses carry a 4-byte `disp32`.
- **`gpr(x)`** — the native register a guest GPR `x` maps to (`conv_reg`); never `rcx`.
- **`<sc>`** — `WIDE_SCRATCH_REG`, a callee-preserved native register `push`/`pop`ed around a wide load/store.
- **Limbs:** i128 = 2, i256 = 4, i512 = 8, i1024 = 16. Inline forms are gated to **≤ i256** unless noted; wider falls back to the trampoline.
- The **96-byte per-instruction native-code budget** (`VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH`) is why the inline forms stop at i256.

---

## Inline forms (default)

### `add` `sub` `and` `or` `xor` — `wide_binop_inline`

Two shapes, chosen by operand aliasing. `add`/`sub` thread the carry/borrow through `adc`/`sbb`; the intervening `mov` does not disturb the flags. `and`/`or`/`xor` carry nothing (the same op on every limb).

**In-place**, when the destination aliases a source (`other` = the non-`d` source; commutative ops may pick either). 2 instr/limb → **8 instr (~56 B)** at i256, fits any width.

`add`, in place (`d += other`):
```asm
mov   rcx, [Wother.0]
add   [Wd.0], rcx
mov   rcx, [Wother.1]
adc   [Wd.1], rcx
mov   rcx, [Wother.2]
adc   [Wd.2], rcx
mov   rcx, [Wother.3]
adc   [Wd.3], rcx
```

`sub`, in place (`d -= other`) — first limb `sub`, then `sbb`:
```asm
mov   rcx, [Wother.0]
sub   [Wd.0], rcx
mov   rcx, [Wother.1]
sbb   [Wd.1], rcx
mov   rcx, [Wother.2]
sbb   [Wd.2], rcx
mov   rcx, [Wother.3]
sbb   [Wd.3], rcx
```

`and`/`or`/`xor`, in place (`xor` shown; `and`→`and`, `or`→`or`) — no carry, same op each limb:
```asm
mov   rcx, [Wother.0]
xor   [Wd.0], rcx
mov   rcx, [Wother.1]
xor   [Wd.1], rcx
mov   rcx, [Wother.2]
xor   [Wd.2], rcx
mov   rcx, [Wother.3]
xor   [Wd.3], rcx
```

**Three-address**, distinct `d`, `s1`, `s2`. 3 instr/limb → **12 instr (88 B)** at i256 (≤ 96 B cap); **i512+ → trampoline**.

`add`, three-address (`d = s1 + s2`):
```asm
mov   rcx, [Ws1.0]
add   rcx, [Ws2.0]
mov   [Wd.0], rcx
mov   rcx, [Ws1.1]
adc   rcx, [Ws2.1]
mov   [Wd.1], rcx
mov   rcx, [Ws1.2]
adc   rcx, [Ws2.2]
mov   [Wd.2], rcx
mov   rcx, [Ws1.3]
adc   rcx, [Ws2.3]
mov   [Wd.3], rcx
```

`sub` three-address is identical with `sub` (limb 0) / `sbb` (limbs 1–3); `and`/`or`/`xor` three-address use their mnemonic on every limb with no carry.

### `move` — `wide_move`

Limb copy; emits **nothing** when `d` aliases `s1`. At i256:
```asm
mov   rcx, [Ws1.0]
mov   [Wd.0], rcx
mov   rcx, [Ws1.1]
mov   [Wd.1], rcx
mov   rcx, [Ws1.2]
mov   [Wd.2], rcx
mov   rcx, [Ws1.3]
mov   [Wd.3], rcx
```

### `trunc` — `wide_truncate` (any width)

The result is `s1`'s low limb — a single load into the destination GPR:
```asm
mov   gpr(d), [Ws1.0]
```

### `zext` / `widen_unsigned` — `wide_widen_unsigned`

Scalar into limb 0, zero the rest. At i256:
```asm
mov   [Wd.0], gpr(s1)
xor   ecx, ecx
mov   [Wd.1], rcx
mov   [Wd.2], rcx
mov   [Wd.3], rcx
```

### `slt_u` / `slt_s` — `wide_slt_inline`

A subtract-with-borrow chain over the file; the final flag is the answer (CF = unsigned borrow; SF≠OF on the top limb = signed less-than). The result GPR is pre-zeroed so `setcc` on its low byte yields a clean 0/1 with no `movzx`; the `mov` loads between the `sbb`s preserve CF. At i256 (~63 B):
```asm
xor   gpr(d), gpr(d)      ; 32-bit, pre-zero result
mov   rcx, [Ws1.0]
sub   rcx, [Ws2.0]
mov   rcx, [Ws1.1]
sbb   rcx, [Ws2.1]
mov   rcx, [Ws1.2]
sbb   rcx, [Ws2.2]
mov   rcx, [Ws1.3]
sbb   rcx, [Ws2.3]
setb  gpr(d)             ; slt_u (Below);  slt_s → setl (Less)
```

### `seq` / `sne` — `wide_equal_inline`

XOR-OR fold, then `setcc` on ZF. The accumulator lives in the result GPR, so a flag-preserving `mov d,0` precedes `setcc`. At i256 (~74 B):
```asm
mov   gpr(d), [Ws1.0]
xor   gpr(d), [Ws2.0]
mov   rcx, [Ws1.1]
xor   rcx, [Ws2.1]
or    gpr(d), rcx
mov   rcx, [Ws1.2]
xor   rcx, [Ws2.2]
or    gpr(d), rcx
mov   rcx, [Ws1.3]
xor   rcx, [Ws2.3]
or    gpr(d), rcx
mov   gpr(d), 0          ; preserves ZF from the final OR
sete  gpr(d)            ; seq (Equal);  sne → setne
```

### `load` / `store` — `wide_access`

The guest address is materialised **once** with `lea` (to fit the byte budget); limbs then index off it.

`load` at i256 (unrolled, 52 B):
```asm
push  <sc>
lea   rcx, [guest_base + offset]
mov   <sc>, [rcx + 0]
mov   [Wd.0], <sc>
mov   <sc>, [rcx + 8]
mov   [Wd.1], <sc>
mov   <sc>, [rcx + 16]
mov   [Wd.2], <sc>
mov   <sc>, [rcx + 24]
mov   [Wd.3], <sc>
pop   <sc>
```

`store` at i256 — identical, with the two `mov`s per limb swapped:
```asm
push  <sc>
lea   rcx, [guest_base + offset]
mov   <sc>, [Ws.0]
mov   [rcx + 0], <sc>
mov   <sc>, [Ws.1]
mov   [rcx + 8], <sc>
mov   <sc>, [Ws.2]
mov   [rcx + 16], <sc>
mov   <sc>, [Ws.3]
mov   [rcx + 24], <sc>
pop   <sc>
```

Wider than i256 walks a counted loop whose size is width-independent (two pointers + counter; **64-bit** pointer adds — a 32-bit `add` here truncated the >4 GiB wide-file pointer and faulted, now fixed). Full body (`load` shown):
```asm
push  <sc>
push  <cnt>
push  <fp>
lea   rcx, [guest_base + offset]
lea   <fp>, [Wr.0]
mov   <cnt>, N            ; N = limb count (8 for i512, 16 for i1024)
body:
  mov   <sc>, [rcx]        ; store: mov <sc>, [<fp>]
  mov   [<fp>], <sc>       ; store: mov [rcx], <sc>
  add   rcx, 8             ; imm64 (REX.W) — 64-bit pointer arithmetic
  add   <fp>, 8
  sub   <cnt>, 1
  jne   body
pop   <fp>
pop   <cnt>
pop   <sc>
```

---

## Trampoline forms

**Ops:** `mul`, `div_u`/`div_s`, `rem_u`/`rem_s`, `exp`, `add_mod`, `mul_mod`, `signext`, `sext_w`, `min_u`/`min_s`, `max_u`/`max_s`, `bswap`, `shl`/`shr_l`/`shr_a` — plus any inline op at i512+. Heavy ops are trampolined by design (µs of real work dwarf the crossing); the shift/`min`/`max`/`bswap`/`sext` group awaits assembler shift and flag-preserving-loop primitives.

**Call site** — `wide_op`. A packed 64-bit descriptor (op · width · d · s1 · s2 · s3) goes in `rcx`; a scalar argument (shifts, `zext`, `sext`) is staged in the VmCtx first; a result (compares, `trunc`) comes back in `rcx`. Full site with both optionals present:
```asm
mov   [vmctx.wide_scalar], gpr(scalar)   ; only for shift / zext / sext
mov   rcx, <descriptor imm64>
call  wide_trampoline
mov   gpr(result), rcx                    ; only for compares / trunc
```
The minimal site (e.g. `mul`, `bswap`, `min`/`max`) is just the middle two instructions:
```asm
mov   rcx, <descriptor imm64>
call  wide_trampoline
```

**Trampoline body** — `emit_wide_trampoline`, emitted **once** per module (so it costs nothing per instruction). `syscall_wide` is a pure helper over the wide file — it never touches the guest GPRs or resumes the guest — so only caller-saved-mapped registers round-trip the VmCtx. Full body:
```asm
wide_trampoline:
  push  rcx
  ; save caller-saved guest regs -> vmctx  (save_caller_saved_registers_to_vmctx)
  mov   rcx, <&syscall_wide>
  pop   rdi                 ; descriptor -> arg0 (System V)
  call  rcx
  push  rax                 ; result
  ; restore caller-saved guest regs <- vmctx
  pop   rcx                 ; result -> rcx
  ret
```

---

## Why not host vector registers (`xmm`/`ymm`)?

An i256 is exactly one 256-bit `ymm` (or two `xmm`), so `load`/`store`/`move`/bitwise *could* be `vmovdqu` pairs — 2 instructions instead of 8 GPR `mov`s. The recompiler emits **no SIMD at all** (grep: zero `xmm`/`ymm`/`vmov` in `compiler/`), for concrete reasons:

1. **The sandbox does not preserve vector state across the guest boundary.** Every guest↔host crossing (the `syscall_wide` trampoline, syscalls, gas-metering interrupts, worker switches) saves/restores only the guest **GPRs** — `save_caller_saved_registers_to_vmctx` iterates `Reg::ALL`, no `xmm`/`ymm`. Recompiled code using vector registers would have that state silently clobbered across any crossing. `cpuid.rs` spells out the hazard: using AVX safely requires the OS to preserve XMM/YMM across context switches — skipping it "silently corrupts state."
2. **AVX2/`ymm` isn't guaranteed.** It needs a three-way check (CPU AVX2 + OS `XSAVE` + OS preserving YMM via `XCR0`); `is_avx2_supported()` exists but is used only for the cache cost-model, not codegen. A vector lowering would need that gate plus an SSE-only fallback.
3. **Arithmetic can't use plain SIMD anyway.** `add`/`sub`/`slt`/`seq` need cross-limb **carry/borrow**, which SSE/AVX2 lack (no carry chain across 64-bit lanes) — the `adc`/`sbb` GPR chain is the natural fit; SIMD would need AVX-512 or manual carry recovery. Only `load`/`store`/`move`/bitwise/`eq` could benefit.
4. **The GPR path already reaches parity.** `load`/`store` are already the cheapest inline ops (~1–2 ns, native), and the whole-contract recompiler result is already **1.00× the base ISA** (§6c) — so vectorizing them is a marginal gain on already-cheap ops, against real cost and a correctness hazard.

So it is a deliberate simplicity + sandbox-ABI choice, not a fundamental limit: a genuine future optimization for `load`/`store`/`move`/bitwise **if** the sandbox grows vector-state preservation and an AVX/SSE gate — but arithmetic stays on GPR carry chains regardless.

## Summary

| wide op | path | x86 shape | native bytes @256 |
|---|---|---|--:|
| `add` `sub` | inline | `mov`+`adc`/`sbb` chain (in-place 8 instr, else 12) | 56 / 88 |
| `and` `or` `xor` | inline | `mov`+`<op>` chain (no carry) | 56 / 88 |
| `move` | inline | `mov`/`mov` per limb (8 instr) | 56 |
| `trunc` | inline | single `mov` (low limb) | 7 |
| `zext` | inline | store limb 0 + `xor` + zero-fill | 30 |
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
