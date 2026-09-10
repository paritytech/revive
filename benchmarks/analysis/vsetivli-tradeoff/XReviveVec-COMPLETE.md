# XReviveVec — complete reference and all experiment data

The wide-integer extension across **revive**, **LLVM (parity-llvm 22.1.8)** and **PolkaVM**: the instruction set, ABI and LMUL support; how the interpreter and recompiler run it; and the full per-benchmark data from every experiment. One x86-64 Linux host; corpus is `-Oz` IR from the in-tree integration contracts.

## 1. The extension

- **`ISA_ReviveV2`**, blob version 4, entirely in RISC-V **`custom-2`** (`custom-1`/`custom-3` stay free). `ReviveV1` is untouched.
- **Width from `vtype`** (set by `vsetivli`): one mnemonic per operation; the PolkaVM linker recovers each width by interprocedural CFG dataflow, and an unresolvable width is a hard error.
- **32 instructions, 7 shapes:** compute (`add sub mul and or xor div·u/s rem·u/s exp signext min/max·u/s bswap move`), shifts (`shl shr·l shr·a`), compares→GPR (`seq sne slt·u/s`), convert (`widen·u/s trunc`), memory (`load store`), fused modular (`addmod mulmod`).

**ABI.** `i256` becomes a machine type in the vector registers (LLVM class `VRM2`), so type legalization stops splitting into `i64` limbs and **wide arguments pass in registers, not by reference** (a 3-wide-arg call: 192-byte frame → 16-byte). Wired into `CC_RISCV` and `CC_RISCV_FastCC`.

**LMUL / widths.** A wide value is a vector register group at VLEN=128: **i128 = LMUL 1, i256 = LMUL 2 (`VRM2`), i512 = LMUL 4, i1024 = LMUL 8**; `vsetivli` selects it, and moves name their own count (`wmv1r/2r/4r/8r`). The corpus is 99.9% i256 (§6).

## 2. LLVM (parity-llvm)

- `+xrevivevec`: `addRegisterClass(MVT::i256)`, ops Legal/Expand, instructions in `custom-2`, a `Select_VRM2` pseudo for wide selects, wide args in the calling conventions.
- Width from `vtype`; the **machine outliner is barred** from `vtype`-dependent code by default (`-riscv-revive-outline-vtype`).
- Reuses RVV: `VMV*R_V` copies, `wld`/`wst` spills, RVV-avoidance so wide and RVV values never share a register. `adc`/`sbb` staged for future inline arithmetic. 3016/3016 CodeGen+MC tests.

## 3. revive / resolc

- `VM_FEATURES` adds `+xrevivevec`. `addmod`/`mulmod`/`exp`/`signextend` emit `llvm.riscv.revive.*` intrinsics (stdlib routines dead-code eliminate).
- Repointed to **polkavm 0.37 + `TargetInstructionSet::ReviveV2`** so the integrated linker emits ReviveV2 (0.35 ICE'd on the encodings). compiler-rt builtins required to build.
- **NewYork IR** (`--newyork`): experimental Yul→IR pipeline; its type inference narrows `i256`→`i64`/`i128` (§8). A codegen fix defaults un-inferred values to `i256`.

## 4. PolkaVM — interpreter and recompiler

| piece | how it works |
|---|---|
| wide register file | 32 slots × 128 bits in `VmCtx`; a wider value spans a run of slots |
| shared arithmetic | one `wide::dispatch` over 64-bit limbs for every width, `u128`-cross-checked; single source of truth for both backends |
| **interpreter** | executes all 32 **inline** in the dispatch loop; wide memory mirrors each memory kind's residency/faulting |
| **recompiler** | **cheap/common compute** (`add`/`sub`/`and`/`or`/`xor`, compares, `trunc`/`zext`/`move`) → **inline native code by default**; **heavy + i512+** → one `syscall_wide` **trampoline** into the shared code; **load/store generated inline** (fault through the guard pages). `POLKAVM_DISABLE_WIDE_INLINE` forces every compute op back onto the trampoline. Trampoline saves only caller-saved-mapped registers |
| sandboxes | generic + Linux **zygote** expose `syscall_wide`; runs on x86-64/BMI2 |
| gas | `CostModel` v3, per-wide-instruction fields, work-proportional naive costs; a serialize/deserialize off-by-one fixed |

## 5. Per-instruction: interpreter vs recompiler (ns/op)

Each wide instruction in a tight loop; the reference is the scalar limb-chain a base-ISA build emits (extension and reference cross-checked equal). `ext` is the extension; `ref` is scalar.

The recompiler column reflects the **current default: cheap/common wide ops inline to native code**; `mul`, shifts, `min`/`max`, `bswap`, `sext_w`/`signext`, and the heavy iterative ops still take the `syscall_wide` **trampoline** (~50–60 ns out-of-line call; div/rem/exp/mod add real limb-loop work). The **path** column marks which. Figures are with sandbox worker logging off (note below), matching `resolc`/`runblob`.

### 256-bit

| op | interp ext | interp ref (scalar) | interp ext÷ref | recomp ext | recomp ref (scalar) | recomp path |
|---|--:|--:|--:|--:|--:|:--|
| `add` | 20.0 | 254.3 | 12.7× | 2.5 | 7.1 | inline |
| `sub` | 20.3 | 261.7 | 12.9× | 2.5 | 7.4 | inline |
| `and` | 20.4 | 117.5 | 5.8× | 2.5 | 2.5 | inline |
| `or` | 20.9 | 122.8 | 5.9× | 2.5 | 2.5 | inline |
| `xor` | 20.5 | 122.2 | 5.9× | 2.5 | 2.5 | inline |
| `slt_u` | 16.7 | 365.5 | 21.9× | 1.3 | 8.9 | inline |
| `slt_s` | 17.2 | — | — | 1.8 | — | inline |
| `seq` | 16.5 | 198.3 | 12.0× | 3.1 | 3.4 | inline |
| `sne` | 16.5 | — | — | 2.2 | — | inline |
| `move` | 13.8 | 58.2 | 4.2× | 1.6 | 1.3 | inline |
| `zext` | 15.2 | 45.2 | 3.0× | 1.2 | 1.2 | inline |
| `trunc` | 10.8 | 14.4 | 1.3× | 0.0 | 0.2 | inline |
| `load` | 28.4 | — | — | 1.9 | — | inline (native) |
| `store` | 16.8 | — | — | 1.9 | — | inline (native) |
| `mul` | 36.4 | 828.2 | 22.7× | 56.9 | 23.0 | trampoline |
| `shl` | 23.9 | 165.1 | 6.9× | 61.3 | 3.8 | trampoline |
| `shr_l` | 24.6 | — | — | 56.5 | — | trampoline |
| `shr_a` | 24.6 | — | — | 57.9 | — | trampoline |
| `min_u` | 18.2 | — | — | 58.1 | — | trampoline |
| `min_s` | 17.8 | — | — | 55.7 | — | trampoline |
| `max_u` | 17.8 | — | — | 58.1 | — | trampoline |
| `max_s` | 18.1 | — | — | 55.8 | — | trampoline |
| `bswap` | 17.5 | 81.6 | 4.7× | 52.5 | 1.9 | trampoline |
| `sext_w` | 12.4 | — | — | 52.4 | — | trampoline |
| `signext` | 21.2 | — | — | 53.7 | — | trampoline |
| `div_u` | 2,807.2 | — | — | 4,489.9 | — | trampoline (heavy) |
| `div_s` | 2,797.9 | — | — | 4,539.8 | — | trampoline (heavy) |
| `rem_u` | 2,781.5 | — | — | 4,522.1 | — | trampoline (heavy) |
| `rem_s` | 2,804.5 | — | — | 4,542.6 | — | trampoline (heavy) |
| `addmod` | 5,574.9 | — | — | 8,996.9 | — | trampoline (heavy) |
| `exp` | 5,551.4 | — | — | 10,191.6 | — | trampoline (heavy) |
| `mulmod` | 6,805.7 | — | — | 10,778.3 | — | trampoline (heavy) |

### 128-bit

| op | interp ext | interp ref (scalar) | interp ext÷ref | recomp ext | recomp ref (scalar) | recomp path |
|---|--:|--:|--:|--:|--:|:--|
| `add` | 17.5 | 137.1 | 7.8× | 1.2 | 3.5 | inline |
| `sub` | 17.5 | 141.9 | 8.1× | 1.1 | 3.5 | inline |
| `and` | 17.4 | 59.5 | 3.4× | 1.2 | 1.1 | inline |
| `or` | 17.4 | 59.9 | 3.4× | 1.2 | 1.3 | inline |
| `xor` | 17.5 | 59.6 | 3.4× | 0.9 | 1.2 | inline |
| `slt_u` | 15.9 | 189.2 | 11.9× | 1.1 | 4.3 | inline |
| `slt_s` | 16.3 | — | — | 0.9 | — | inline |
| `seq` | 15.8 | 105.2 | 6.7× | 1.1 | 2.1 | inline |
| `sne` | 15.8 | — | — | 1.1 | — | inline |
| `move` | 13.1 | 28.9 | 2.2× | 0.7 | 0.6 | inline |
| `zext` | 15.0 | 30.2 | 2.0× | 0.5 | 0.5 | inline |
| `trunc` | 10.7 | 16.1 | 1.5× | 0.1 | 0.2 | inline |
| `load` | 20.5 | — | — | 1.2 | — | inline (native) |
| `store` | 13.4 | — | — | 1.2 | — | inline (native) |
| `mul` | 23.9 | 252.4 | 10.6× | 56.5 | 5.2 | trampoline |
| `shl` | 21.4 | 65.1 | 3.0× | 57.3 | 1.4 | trampoline |
| `shr_l` | 21.3 | — | — | 59.6 | — | trampoline |
| `shr_a` | 21.3 | — | — | 59.0 | — | trampoline |
| `min_u` | 17.1 | — | — | 58.1 | — | trampoline |
| `min_s` | 16.8 | — | — | 55.6 | — | trampoline |
| `max_u` | 16.8 | — | — | 58.2 | — | trampoline |
| `max_s` | 17.1 | — | — | 55.7 | — | trampoline |
| `bswap` | 16.3 | 40.6 | 2.5× | 53.6 | 0.9 | trampoline |
| `sext_w` | 12.4 | — | — | 52.3 | — | trampoline |
| `signext` | 22.1 | — | — | 53.7 | — | trampoline |
| `div_u` | 654.1 | — | — | 1,025.7 | — | trampoline (heavy) |
| `div_s` | 685.2 | — | — | 1,060.2 | — | trampoline (heavy) |
| `rem_u` | 670.0 | — | — | 1,032.8 | — | trampoline (heavy) |
| `rem_s` | 672.8 | — | — | 1,068.0 | — | trampoline (heavy) |
| `addmod` | 1,280.9 | — | — | 1,932.6 | — | trampoline (heavy) |
| `exp` | 1,611.8 | — | — | 4,127.0 | — | trampoline (heavy) |
| `mulmod` | 1,898.2 | — | — | 3,155.8 | — | trampoline (heavy) |

**Reading it.** *Interpreter:* one wide op replaces a whole scalar limb chain, so the extension wins across the board (3–28× at 256-bit; smaller at 128-bit — a shorter chain to replace). *Recompiler:* the **inlined ops now run at or below the scalar reference** — `add` 2.5 ns vs 7.1, `slt_u` 1.3 vs 8.9, `trunc`/`zext` at or under 1 ns, bitwise at parity — because they generate native code with no call. This is the key change from the earlier trampoline-for-everything snapshot, where every wide op was ~50–60 ns and so uniformly slower than the recompiler's few-ns inline scalar. The **still-trampolined ops** (`mul`, shifts, `min`/`max`, `bswap`, `sext`) cost that ~50–60 ns crossing and remain a per-op loss versus inline scalar — the open inlining work in §10. The **heavy iterative ops** (`div`/`rem`/`exp`/`mod`) are genuinely thousands of ns of limb-loop work; the trampoline is a rounding error on them, and there is no short scalar form to beat. Whole-contract impact is §6c/§6d, where the inlined cheap ops dominate and the recompiler reaches 1.00× ref.

> **Measurement note.** `wide_microbench.rs` runs as a `#[test]`, where `is_sandbox_logging_enabled()` (`cfg!(test) || …`) forces worker trace logging on — adding ~1,680 ns of `write()` I/O per op **to every trampoline crossing** (a flat ~1,700 ns column if left on), but not to inlined ops or the interpreter. The recompiler figures were captured with that gate forced off, matching `resolc`/`runblob`; §6 (`runblob`, not a test build) was never affected.

### 5a. Per-instruction gas (deterministic): extension vs scalar

The gas analogue of the ns/op tables — what a chain charges, deterministic and backend-independent. `ext gas` is the wide op's `WIDE_*` cost (§6d); `ref gas` is the scalar limb chain a base-ISA build emits (1 gas/instruction). Measured by `wide_microbench_gas`; data in `microbench-gas.txt`. **Recalibrated** so every one-64-bit-op-per-limb wide op costs `N × 1 = 4` at the 256-bit width (N = 4 limbs) — never more than the scalar chain it replaces (`WIDE_LINEAR 16→4`, `WIDE_MEMORY 6→4`; §6d). Superlinear ops keep their costs.

| op | ext | ref @256 | ext÷ref @256 | ref @128 | ext÷ref @128 |
|---|--:|--:|--:|--:|--:|
| `add`/`sub` | 4 | 33 | **0.12×** | 17 | 0.24× |
| `and`/`or`/`xor` | 4 | 16 | **0.25×** | 8 | 0.50× |
| `slt_u` | 4 | 43 | **0.09×** | 23 | 0.17× |
| `seq` | 4 | 22 | **0.18×** | 14 | 0.29× |
| `shl` | 4 | 21 | **0.19×** | 9 | 0.44× |
| `bswap` | 4 | 12 | **0.33×** | 6 | 0.67× |
| `move` | 4 | 8 | **0.50×** | 4 | 1.00× |
| `load`/`store` | 4 | ≈4 (4 limbs) | **≈1.0×** | ≈2 | ≈2× |
| `zext` | 2 | 6 | **0.33×** | 4 | 0.50× |
| `trunc` | 2 | 2 | 1.00× | 2 | 1.00× |
| `min`/`max`/`sext`/`signext`/shifts | 4 (2 for `sext`) | — | — | — | — |
| `mul` | 56 | 105 | **0.53×** | 29 | 1.93× |
| `div_u`/`rem_u` | 3,100 | — | — | — | — |
| `div_s`/`rem_s` | 3,200 | — | — | — | — |
| `mul_mod`/`add_mod` | 6,300 | — | — | — | — |
| `exp` | 8,000 | — | — | — | — |

**After the recalibration, at 256-bit every linear wide op is gas-cheaper than the scalar chain it replaces** (0.09×–0.53×), and `load`/`store` are now **≈neutral** (4 vs ~4 limb loads) instead of the earlier 1.5× that had driven the whole-contract increase (§6d/§12). The only ratio above 1 is `mul` at **128-bit** (1.93×) — it is superlinear (O(N²)) so the `N × 64-bit` ceiling doesn't apply, and its single width-independent cost (56) is calibrated to the 256-bit chain (105), where it is 0.53×; i128 is unused in practice (§7).

## 5b. Recompiler native-code size per instruction (the per-instruction JIT budget)

The recompiler emits native x86-64 per guest instruction into a slot bounded by **`VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH` = 96 B** (raised from 69 to admit the inline wide lowerings). This is a **runner-side JIT limit — not the VM ABI or the on-chain blob**: a wide op is one blob instruction however the recompiler lowers it. The bytes below are per-instruction emission on the **trampoline path** (the fallback, forced by `POLKAVM_DISABLE_WIDE_INLINE`): compute ops are a fixed call site (the shared trampoline body is emitted once, off-budget) so are width-independent; load/store are inline and grow with width. The cheap/common compute ops now default to inline native code instead (table below, §6c).

| instruction | opcode | emit path | bytes @128 | bytes @256 | ≤ 96 B |
|---|--:|---|--:|--:|:--:|
| `add` | 0 | trampoline | 10 | 10 | ✅ |
| `sub` | 1 | trampoline | 10 | 10 | ✅ |
| `mul` | 2 | trampoline | 10 | 10 | ✅ |
| `and` | 3 | trampoline | 10 | 10 | ✅ |
| `or` | 4 | trampoline | 10 | 10 | ✅ |
| `xor` | 5 | trampoline | 10 | 10 | ✅ |
| `div_u` | 6 | trampoline | 10 | 10 | ✅ |
| `div_s` | 7 | trampoline | 10 | 10 | ✅ |
| `rem_u` | 8 | trampoline | 10 | 10 | ✅ |
| `rem_s` | 9 | trampoline | 10 | 10 | ✅ |
| `exp` | 10 | trampoline | 10 | 10 | ✅ |
| `signext` | 11 | trampoline | 10 | 10 | ✅ |
| `min_u` | 12 | trampoline | 10 | 10 | ✅ |
| `min_s` | 13 | trampoline | 10 | 10 | ✅ |
| `max_u` | 14 | trampoline | 10 | 10 | ✅ |
| `max_s` | 15 | trampoline | 10 | 10 | ✅ |
| `shl` | 16 | trampoline (+scalar store) | 17 | 17 | ✅ |
| `shr_l` | 17 | trampoline (+scalar store) | 17 | 17 | ✅ |
| `shr_a` | 18 | trampoline (+scalar store) | 17 | 17 | ✅ |
| `seq` | 19 | trampoline (+result mov) | 13 | 13 | ✅ |
| `sne` | 20 | trampoline (+result mov) | 13 | 13 | ✅ |
| `slt_u` | 21 | trampoline (+result mov) | 13 | 13 | ✅ |
| `slt_s` | 22 | trampoline (+result mov) | 13 | 13 | ✅ |
| `trunc` | 23 | trampoline (+result mov) | 13 | 13 | ✅ |
| `zext` | 24 | trampoline (+scalar store) | 17 | 17 | ✅ |
| `sext_w` | 25 | trampoline (+scalar store) | 17 | 17 | ✅ |
| `bswap` | 26 | trampoline | 10 | 10 | ✅ |
| `move` | 27 | trampoline | 10 | 10 | ✅ |
| `addmod` | 28 | trampoline | 15 | 15 | ✅ |
| `mulmod` | 29 | trampoline | 15 | 15 | ✅ |
| `load` | 30 | **inline** | 30 | 52 | ✅ |
| `store` | 31 | **inline** | 30 | 52 | ✅ |

Every instruction as currently implemented fits the budget with headroom (max = load/store at 52 B for i256). The trampoline keeps compute ops tiny at the call site (10–17 B) at the cost of the runtime crossing measured in §5. Load/store are unrolled per limb up to i256; **above i256 they use a fixed-size copy loop** (two pointers and a counter) so the emission stays within budget at any width — a bug where that loop advanced its pointers with a 32-bit `add` (truncating the wide-file pointer, a VMCTX address above 4 GiB, and trapping i512+ wide memory on the recompiler) is now fixed (§10).

**Inlining the wide compute ops (now the default; kill switch `POLKAVM_DISABLE_WIDE_INLINE`).** The recompiler emits the cheap/common wide ops as inline native code (only `rcx`, nothing spills), falling back to the trampoline for heavy and i512+ ops. At i256: `add`/`sub`/`and`/`or`/`xor` — *in-place* 2 instr/limb when the destination aliases a source (`add`/`sub` chain the carry/borrow through `adc`/`sbb`, the `mov` preserving flags; any width), else *three-address* 3 instr/limb (i256 = 88 B, under the cap); `trunc` — low-limb `mov` (any width); `zext` — scalar into limb 0, zero the rest (≤i256); `move` — limb copy, no-op when aliased (≤i256).

Measured native bytes and correctness — every case cross-checked bit-for-bit against the scalar reference (`= yes`):

| op(s) | shape | bytes @128 | bytes @256 | ≤ 96 B | inline ns/op | vs ~55 ns trampoline |
|---|---|--:|--:|:--:|--:|--:|
| `trunc` | low-limb load → GPR | 7 | 7 | ✅ | ~0.0–0.1 | ~500×+ |
| `zext` | store + zero-fill | 16 | 30 | ✅ | ~0.4–0.6 | ~90–140× |
| `move` | limb copy | 28 | 56 | ✅ | ~2.7–3.9 | ~14–20× |
| `add`/`sub`/`and`/`or`/`xor`, in-place | 2/limb | 28 | 56 | ✅ | ~2–9 | ~6–27× |
| `add`/`sub`/`and`/`or`/`xor`, distinct | 3/limb | 42 | 88 | ✅ | ~2–4 | ~14–27× |

**Which of the 32 inline vs stay on the trampoline:**

- **Inlined (≤ i256, any register layout)** — `trunc`, `zext`, `move`; `add`/`sub`/`and`/`or`/`xor` (in-place 2 instr/limb when the destination aliases a source, else three-address 3 instr/limb — i256 = 88 B); `slt_u`/`slt_s` via a subtract-with-borrow chain that leaves the answer in the carry/sign flag (63 B); `seq`/`sne` via an XOR-OR fold (~74 B). After raising the per-instruction cap to 96 B (below), **all the cheap/common wide ops now inline at i256** — together with the already-inline `wld`/`wst`, that is the large majority of wide ops.
- **Trampoline — heavy by design** (µs of arithmetic dwarf the crossing; inlining is pointless): `mul`, `div_u/s`, `rem_u/s`, `exp`, `addmod`, `mulmod`, `signextend`.
- **Trampoline — needs an assembler primitive**: `shl`/`shr_l`/`shr_a` and `sext` (the assembler has **no shift instruction**); `min`/`max` (compare-and-select, branchy); `bswap` (byte-reverse plus limb reversal).
- **Trampoline — i512/i1024**: the inline forms are gated to ≤ i256 (an i512 chain is ≥ 112 B, over the 96-byte cap). The corpus is 99.9% i256, so this is moot in practice.

### PVM limitations that shape this

What inlines is bounded by concrete recompiler constraints, not effort:

1. **96 B per-instruction budget** (`VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH`, raised from 69) — it sizes a runner-side, non-ABI native-code arena (`≥ VM_MAXIMUM_CODE_SIZE × 96`) that must sit below the 4 GiB ceiling the zygote asserts. Admits i256 distinct-register arith (88 B) and i256 `seq`/`sne` (~74 B); i512+ (≥112 B) exceeds it.
2. **One general scratch (`rcx`)** plus three push/pop-preserved wide temporaries, and a deep `VmCtx` layout forcing 4-byte `disp32` limb offsets (no free register for a nearer base pointer) — so a distinct 3-address i256 op is ~12 memory instructions ≈ 84 B.
3. **No shift and no flag-preserving loop** in the assembler — so wide shifts, `sext`'s sign mask, and a width-independent carry loop can't be built.
4. **Overlap safety** — inline forms compute in place (operands must be identical or disjoint, always true for well-formed codegen); the trampoline copies to temporaries and tolerates any overlap.

Net: at i256 the cheap/common majority inline (`trunc`/`zext`/`move`, `add`/`sub`/`and`/`or`/`xor`, `slt_u/s`/`seq`/`sne`); heavy ops stay on the trampoline by design, shifts/`min`/`max`/`bswap`/`sext` until the assembler grows the primitives, i512+ until a further cap raise. **Whole-contract effect (§6c): 1.00× ref on the recompiler, −23% vs the trampoline.**

## 6. Per-benchmark: ref vs vsetvli+MO vs vsetvli-noMO

All 64 contracts that compiled, linked and ran in every arm. Execution is `runblob`'s amortized per-call time (instantiate once, rerun 50×), so JIT and sandbox spawn are netted out.

### 6a. Code size (bytes)

| benchmark | blob ref | blob MO | blob noMO | noMO/ref | .text ref | .text MO | .text noMO |
|---|--:|--:|--:|--:|--:|--:|--:|
| ERC20.sol.ERC20 | 20,793 | 13,007 | 10,693 | -48.6% | 15,228 | 9,892 | 10,724 |
| EncodePackedHash.sol.EncodePackedHash | 7,495 | 5,072 | 4,711 | -37.1% | 5,636 | 4,764 | 4,806 |
| KeccakFuseBug.sol.KeccakFuseBug | 5,951 | 4,538 | 4,441 | -25.4% | 4,336 | 4,076 | 4,078 |
| MemoryBounds.sol.MemoryBounds | 5,171 | 4,079 | 3,753 | -27.4% | 3,962 | 3,740 | 3,758 |
| MCopyOverlap.sol.MCopyOverlap | 3,405 | 3,519 | 3,407 | +0.1% | 2,796 | 3,256 | 3,268 |
| CallerOriginAliasing.sol.CallerOriginAliasing | 4,739 | 3,599 | 3,089 | -34.8% | 3,476 | 3,396 | 3,660 |
| ExtCode.sol.ExtCode | 3,936 | 3,441 | 3,051 | -22.5% | 3,014 | 2,994 | 3,060 |
| Storage.sol.Storage | 3,501 | 2,955 | 2,955 | -15.6% | 2,740 | 2,768 | 2,768 |
| CallGas.sol.Other | 3,381 | 3,048 | 2,856 | -15.5% | 2,874 | 2,854 | 2,866 |
| Fibonacci.sol.FibonacciBinet | 4,505 | 2,860 | 2,790 | -38.1% | 3,606 | 2,774 | 2,776 |
| Call.sol.Callee | 3,476 | 2,747 | 2,747 | -21.0% | 2,896 | 2,828 | 2,828 |
| DelegateCaller.sol.DelegateCaller | 3,076 | 2,803 | 2,708 | -12.0% | 2,580 | 2,660 | 2,662 |
| MCopy.sol.MCopy | 3,452 | 2,706 | 2,706 | -21.6% | 2,880 | 2,810 | 2,810 |
| ReturnDataOob.sol.Callee | 3,452 | 2,706 | 2,706 | -21.6% | 2,878 | 2,810 | 2,810 |
| Immutables.sol.ImmutablesTester | 3,550 | 2,927 | 2,695 | -24.1% | 2,870 | 2,786 | 2,804 |
| FunctionPointer.sol.FunctionPointer | 3,254 | 2,961 | 2,678 | -17.7% | 2,586 | 2,650 | 2,670 |
| TryCatchCatchReturn.sol.TryCatchCatchReturn | 3,055 | 2,712 | 2,617 | -14.3% | 2,506 | 2,556 | 2,558 |
| Transfer.sol.Transfer | 2,821 | 2,487 | 2,487 | -11.8% | 2,310 | 2,418 | 2,418 |
| LayoutAt.sol.LayoutAt | 2,928 | 2,491 | 2,426 | -17.1% | 2,408 | 2,432 | 2,444 |
| FunctionType.sol.FunctionType | 2,664 | 2,534 | 2,415 | -9.3% | 2,254 | 2,394 | 2,402 |
| AddressPredictor.sol.Predicted | 2,736 | 2,397 | 2,397 | -12.4% | 2,296 | 2,394 | 2,394 |
| AddModMulMod.sol.AddModMulMod | 2,866 | 2,681 | 2,347 | -18.1% | 2,424 | 2,446 | 2,480 |
| SubTypeValidation.sol.SubTypeValidation | 2,614 | 2,556 | 2,294 | -12.2% | 2,284 | 2,342 | 2,360 |
| Context.sol.Context | 2,776 | 2,560 | 2,257 | -18.7% | 2,272 | 2,364 | 2,388 |
| FmpCrossObjectBug.sol.FmpCrossObjectBug | 2,440 | 2,305 | 2,209 | -9.5% | 2,078 | 2,200 | 2,202 |
| RevertDataOob.sol.RevertDataOob | 2,694 | 2,262 | 2,191 | -18.7% | 2,102 | 2,176 | 2,196 |
| Fibonacci.sol.FibonacciRecursive | 2,705 | 2,282 | 2,188 | -19.1% | 2,358 | 2,292 | 2,294 |
| Block.sol.Block | 2,430 | 2,414 | 2,159 | -11.2% | 2,106 | 2,238 | 2,244 |
| Factorial.sol.Factorial | 2,433 | 2,189 | 2,091 | -14.1% | 2,162 | 2,244 | 2,246 |
| FmpRangeProofBug.sol.FmpRangeProofBug | 2,260 | 2,085 | 2,085 | -7.7% | 1,976 | 2,126 | 2,126 |
| CopyOverlapBug.sol.CopyOverlapBug | 2,285 | 2,143 | 2,048 | -10.4% | 2,036 | 2,164 | 2,166 |
| BlockHash.sol.BlockHash | 2,385 | 2,037 | 2,037 | -14.6% | 2,112 | 2,222 | 2,222 |
| FmpNativeStoreBug.sol.FmpNativeStoreBug | 2,408 | 2,022 | 2,022 | -16.0% | 2,168 | 2,146 | 2,146 |
| UnalignedMStore8Bug.sol.UnalignedMStore8Bug | 2,224 | 2,218 | 2,018 | -9.3% | 1,998 | 2,124 | 2,130 |
| CustomErrorArgs.sol.CustomErrorArgs | 2,301 | 2,059 | 2,015 | -12.4% | 2,050 | 2,088 | 2,090 |
| MStore8.sol.MStore8 | 2,200 | 2,082 | 1,985 | -9.8% | 1,984 | 2,124 | 2,126 |
| ParamMload.sol.ParamMload | 2,211 | 2,068 | 1,972 | -10.8% | 1,970 | 2,112 | 2,114 |
| UnalignedMStoreBug.sol.UnalignedMStoreBug | 2,059 | 2,076 | 1,940 | -5.8% | 1,928 | 2,046 | 2,050 |
| FmpDynStoreBug.sol.FmpDynStoreBug | 2,103 | 1,893 | 1,893 | -10.0% | 1,888 | 2,004 | 2,004 |
| UnalignedMloadNativeBug.sol.UnalignedMload | 2,030 | 2,022 | 1,886 | -7.1% | 1,912 | 2,020 | 2,024 |
| MLoad.sol.MLoad | 2,066 | 1,880 | 1,880 | -9.0% | 1,940 | 2,034 | 2,034 |
| Events.sol.Events | 2,160 | 1,926 | 1,823 | -15.6% | 1,892 | 1,880 | 1,882 |
| Library.sol.L | 1,876 | 1,864 | 1,706 | -9.1% | 1,724 | 1,858 | 1,872 |
| Send.sol.Send | 1,904 | 1,695 | 1,695 | -11.0% | 1,766 | 1,848 | 1,848 |
| Transaction.sol.TransactionOrigin | 1,750 | 1,768 | 1,627 | -7.0% | 1,704 | 1,962 | 1,966 |
| StructDeleteStorage.sol.StructDeleteStorage | 1,703 | 1,676 | 1,602 | -5.9% | 1,632 | 1,740 | 1,742 |
| Fibonacci.sol.FibonacciIterative | 1,872 | 1,550 | 1,481 | -20.9% | 1,796 | 1,720 | 1,722 |
| FmpDynRevertBug.sol.FmpDynRevertBug | 1,597 | 1,481 | 1,481 | -7.3% | 1,492 | 1,612 | 1,612 |
| SubUnderflowZext.sol.SubUnderflowZext | 1,519 | 1,543 | 1,471 | -3.2% | 1,548 | 1,722 | 1,724 |
| ConstReturnOverflowBug.sol.ConstReturnOverflowBug | 1,402 | 1,523 | 1,397 | -0.4% | 1,404 | 1,542 | 1,546 |
| Value.sol.ValueTester | 1,418 | 1,357 | 1,357 | -4.3% | 1,456 | 1,562 | 1,562 |
| PanicInterveneBug.sol.PanicInterveneBug | 1,384 | 1,468 | 1,349 | -2.5% | 1,456 | 1,546 | 1,554 |
| PanicCodeBug.sol.PanicCodeBug | 1,353 | 1,454 | 1,339 | -1.0% | 1,434 | 1,526 | 1,534 |
| Baseline.sol.Baseline | 1,273 | 1,261 | 1,261 | -0.9% | 1,374 | 1,502 | 1,502 |
| FmpRevertBug.sol.FmpRevertBug | 1,295 | 1,337 | 1,238 | -4.4% | 1,408 | 1,512 | 1,520 |
| Coinbase.sol.Coinbase | 1,254 | 1,215 | 1,215 | -3.1% | 1,370 | 1,478 | 1,478 |
| Selfdestruct.sol.SelfdestructTester | 1,254 | 1,211 | 1,211 | -3.4% | 1,324 | 1,416 | 1,416 |
| BaseFee.sol.BaseFee | 1,228 | 1,165 | 1,165 | -5.1% | 1,354 | 1,426 | 1,426 |
| GasLeft.sol.GasLeft | 1,212 | 1,162 | 1,162 | -4.1% | 1,350 | 1,414 | 1,414 |
| GasPrice.sol.GasPrice | 1,194 | 1,148 | 1,148 | -3.9% | 1,342 | 1,406 | 1,406 |
| GasLimit.sol.GasLimit | 1,191 | 1,141 | 1,141 | -4.2% | 1,336 | 1,400 | 1,400 |
| Create.sol.CreateA | 1,035 | 989 | 989 | -4.4% | 1,226 | 1,300 | 1,300 |
| Create2.sol.CreateA | 1,035 | 989 | 989 | -4.4% | 1,226 | 1,300 | 1,300 |
| Balance.sol.BalanceReceiver | 1,033 | 988 | 988 | -4.4% | 1,226 | 1,300 | 1,300 |
| **TOTAL (64)** | **177,773** | **151,334** | **142,680** | **-19.7%** | **151,720** | **148,736** | **150,232** |

### 6b. Compile time (llc)

`llc` best-of-3, aggregate over the 95 modules that compiled in all three arms. Execution time is in §6c and gas in §6d; the interpreter/recompiler execution columns that were here before have been removed — they predated the harness fix and were inflated by the gas-cap spin artifact.

| arm | total llc compile (s) | vs ref |
|---|--:|--:|
| ref (no extension) | 4.1 | 1.00× |
| vsetvli + MO | 3.8 | 0.92× |
| vsetvli − MO | 3.8 | 0.91× |

The extension arms compile within noise of the base ISA (best-of-3 `llc` is itself ±a few %). Per-module code size is in §6a; the machine outliner is disabled regardless (§8).

### 6c. Recompiler execution: ref vs extension (trampoline vs inline)

Per-call recompiler time (µs), 64 modules, **median of `min`-of-300 across 5 sweeps** per arm. These ~2–3 µs times are dominated by per-call harness overhead (instance re-entry, host-call round-trips), swamping the sub-µs the extension changes — **so per-contract rows carry ~±15% noise, only the aggregate is reliable**; the deterministic per-contract cost is gas (§6d).

| | recomp total (ms) | vs ref |
|---|--:|--:|
| ref (no extension) | 0.19 | 1.00× |
| ext — trampoline | 0.24 | 1.30× |
| **ext — inline** | **0.19** | **1.00×** |

ext-inline is **1.00× ref aggregate, 1.00× median** — parity. Only 3/64 contracts reach 1.10–1.11×, all memory-op modules with no wide arithmetic (ratios within the ±15% floor); the extension carries no compute regression once the cheap/common wide ops inline.

**Inlining erased the ERC20 regression.** ERC20 (256-bit balance load/store/`move`, statically 21 `slt_u` + 37 `seq`) was 1.15× ref on the trampoline; inlining `slt_u/s`, then `seq`/`sne` and distinct-register arith (after the 96 B cap raise), closed it to **1.03×** — parity within the noise floor. The residual is the wide file's memory residency (166 loads + 135 stores; the §7 narrowing transform would remove it), the same reason gas stays ~1.3× ref.

Full per-benchmark table (µs, median-of-5-sweeps; treat individual rows as ±15%):

| benchmark | ref | ext (tramp) | ext-inline | ext-inline ÷ ref |
|---|--:|--:|--:|--:|
| UnalignedMloadNativeBug.UnalignedMload | 2.5 | 3.3 | 2.8 | 1.11× |
| UnalignedMStoreBug.UnalignedMStoreBug | 2.8 | 2.9 | 3.1 | 1.10× |
| MemoryBounds.MemoryBounds | 3.0 | 3.9 | 3.3 | 1.10× |
| ERC20.ERC20 | 3.8 | 5.7 | 3.9 | 1.03× |
| CustomErrorArgs.CustomErrorArgs | 2.8 | 3.7 | 2.9 | 1.01× |
| FmpRevertBug.FmpRevertBug | 2.9 | 3.9 | 3.0 | 1.01× |
| Coinbase.Coinbase | 2.7 | 4.1 | 2.8 | 1.01× |
| SubUnderflowZext.SubUnderflowZext | 2.8 | 3.7 | 2.9 | 1.01× |
| UnalignedMStore8Bug.UnalignedMStore8Bug | 2.8 | 3.7 | 2.9 | 1.01× |
| Selfdestruct.SelfdestructTester | 2.9 | 3.4 | 2.9 | 1.01× |
| Create2.CreateA | 2.7 | 3.1 | 2.7 | 1.00× |
| Immutables.ImmutablesTester | 2.7 | 3.6 | 2.7 | 1.00× |
| MLoad.MLoad | 2.7 | 3.2 | 2.7 | 1.00× |
| AddModMulMod.AddModMulMod | 2.8 | 3.7 | 2.8 | 1.00× |
| AddressPredictor.Predicted | 2.9 | 3.7 | 2.9 | 1.00× |
| FmpDynRevertBug.FmpDynRevertBug | 3.2 | 4.3 | 3.2 | 1.00× |
| Balance.BalanceReceiver | 2.7 | 3.1 | 2.7 | 1.00× |
| FmpNativeStoreBug.FmpNativeStoreBug | 3.3 | 4.7 | 3.3 | 1.00× |
| Baseline.Baseline | 2.8 | 3.2 | 2.8 | 1.00× |
| Events.Events | 2.8 | 3.7 | 2.9 | 1.00× |
| TryCatchCatchReturn.TryCatchCatchReturn | 2.8 | 3.7 | 2.9 | 1.00× |
| BaseFee.BaseFee | 2.7 | 4.1 | 2.7 | 1.00× |
| FmpRangeProofBug.FmpRangeProofBug | 3.5 | 5.0 | 3.5 | 1.00× |
| ConstReturnOverflowBug.ConstReturnOverflowBug | 2.8 | 3.3 | 2.8 | 1.00× |
| Factorial.Factorial | 2.9 | 3.7 | 2.9 | 1.00× |
| PanicInterveneBug.PanicInterveneBug | 3.0 | 4.8 | 3.0 | 1.00× |
| StructDeleteStorage.StructDeleteStorage | 2.8 | 3.7 | 2.8 | 1.00× |
| Value.ValueTester | 2.7 | 3.6 | 2.7 | 1.00× |
| Fibonacci.FibonacciIterative | 2.8 | 3.7 | 2.8 | 1.00× |
| MCopyOverlap.MCopyOverlap | 2.9 | 3.7 | 2.9 | 1.00× |
| SubTypeValidation.SubTypeValidation | 2.8 | 3.7 | 2.8 | 1.00× |
| LayoutAt.LayoutAt | 3.1 | 4.0 | 3.1 | 1.00× |
| PanicCodeBug.PanicCodeBug | 3.0 | 4.4 | 3.0 | 1.00× |
| BlockHash.BlockHash | 2.7 | 3.1 | 2.7 | 1.00× |
| CopyOverlapBug.CopyOverlapBug | 2.9 | 3.7 | 2.8 | 1.00× |
| MCopy.MCopy | 2.8 | 3.2 | 2.8 | 1.00× |
| Fibonacci.FibonacciRecursive | 2.9 | 3.8 | 2.8 | 1.00× |
| GasPrice.GasPrice | 2.8 | 3.2 | 2.8 | 1.00× |
| ReturnDataOob.Callee | 2.8 | 3.3 | 2.8 | 1.00× |
| ExtCode.ExtCode | 2.9 | 3.7 | 2.8 | 1.00× |
| FmpCrossObjectBug.FmpCrossObjectBug | 3.7 | 5.5 | 3.7 | 0.99× |
| KeccakFuseBug.KeccakFuseBug | 3.3 | 4.2 | 3.3 | 0.99× |
| Context.Context | 2.9 | 3.8 | 2.8 | 0.99× |
| ParamMload.ParamMload | 2.9 | 3.7 | 2.8 | 0.99× |
| EncodePackedHash.EncodePackedHash | 2.9 | 3.7 | 2.9 | 0.99× |
| Block.Block | 2.8 | 3.7 | 2.8 | 0.99× |
| GasLimit.GasLimit | 2.8 | 3.3 | 2.8 | 0.99× |
| FmpDynStoreBug.FmpDynStoreBug | 3.2 | 5.1 | 3.2 | 0.99× |
| Transaction.TransactionOrigin | 2.9 | 3.7 | 2.8 | 0.99× |
| DelegateCaller.DelegateCaller | 2.9 | 3.7 | 2.8 | 0.99× |
| Send.Send | 2.7 | 3.6 | 2.7 | 0.99× |
| Call.Callee | 2.8 | 3.2 | 2.8 | 0.99× |
| Transfer.Transfer | 2.7 | 3.6 | 2.7 | 0.99× |
| Library.L | 3.1 | 4.4 | 3.1 | 0.99× |
| FunctionPointer.FunctionPointer | 3.2 | 4.0 | 3.1 | 0.99× |
| MStore8.MStore8 | 2.9 | 3.7 | 2.8 | 0.99× |
| Storage.Storage | 2.9 | 3.7 | 2.9 | 0.99× |
| RevertDataOob.RevertDataOob | 2.9 | 3.7 | 2.8 | 0.99× |
| Create.CreateA | 2.7 | 3.1 | 2.7 | 0.99× |
| FunctionType.FunctionType | 3.1 | 4.4 | 3.1 | 0.98× |
| Fibonacci.FibonacciBinet | 2.9 | 3.7 | 2.8 | 0.98× |
| CallerOriginAliasing.CallerOriginAliasing | 2.9 | 3.7 | 2.8 | 0.98× |
| CallGas.Other | 3.7 | 5.4 | 3.6 | 0.98× |
| GasLeft.GasLeft | 3.0 | 3.3 | 2.6 | 0.87× |
| **TOTAL (64)** | **187** | **244** | **187** | **1.00×** |

### 6d. Gas: extension vs base ISA (after recalibration)

**How gas is charged (code path).** A `CostModel` (`polkavm/src/gas.rs`) holds one `Cost` per opcode; the default `naive()` is 1 gas/scalar-op plus work-proportional wide costs (`with_wide_costs`: `WIDE_MEMORY=4`, `WIDE_LINEAR=4`, `WIDE_CONVERT=2`, `WIDE_MULTIPLY=56`, …). At module build a `GasVisitor` walks the code and sums `cost_for_opcode` into a **per-basic-block** cost (`calculate_for_block`). At runtime each block's cost is deducted **on entry**: the interpreter's `charge_gas` handler (`gas -= block_cost`, `NotEnoughGas` if short), and the recompiler's per-block `emit_gas_metering_stub` (`sub [vmctx.gas], block_cost`, read back by `extract_gas_cost`). Same cost model + same block sums on both backends ⇒ gas is **deterministic, backend-independent, and independent of inline vs trampoline** (codegen doesn't change a block's opcode set). `GasMeteringKind::Sync` (used here) aborts at the overrunning block.

Gas is what a chain charges. Two recalibrations brought it down: first removing an old fixed `WIDE_MARSHAL = 8` marshal term (which over-metered cheap ops 6–24× → **+2.47×**), then capping every **one-64-bit-op-per-limb** wide op at `N × (64-bit op cost) = N × 1 = 4` at the 256-bit width (N = 4 limbs) — so a wide op never costs more than doing it limb-by-limb in scalar. Superlinear ops (mul O(N²), div/rem/mod/exp iterative) can't be bounded by `N × 64-bit` and keep costs ≤ their real scalar chains.

| wide op class | marshal-era | interim | **now** | scalar chain @256 (§5a) |
|---|--:|--:|--:|--:|
| `trunc`/`zext`/`sext` (convert) | 12 | 2 | **2** | 2–6 |
| `move` | 24 | 4 | **4** | 8 |
| `load`/`store` (memory) | 24 | 6 | **4** | ~4 |
| `add`/`sub`/bitwise/min/max/cmp/shift/`bswap` (linear) | 24 | 16 | **4** | 12–43 |
| `mul` (superlinear, exempt) | 64 | 56 | **56** | 105 |
| `div`/`rem` | 3100/3200 | 3100/3200 | **3100/3200** | ~2,600–4,000 |
| `addmod`/`mulmod` | 6300 | 6300 | **6300** | ~5,000 |
| `exp` | 8000 | 8000 | **8000** | ~8,000 |

Every non-superlinear class is now `≤ N × 64-bit` (4), and each is ≤ its measured scalar chain (§5a). The iterative ops keep their large absolute costs — a division or modular multiply is genuinely thousands of primitive ops.

**Result:** aggregate ext/ref gas: **2.47× → 1.18× (interim) → 1.07× (now)**. On real contracts (§12) it goes to **0.993×** — below the base ISA.

**Residual +7% decomposed** (`runblob RUNBLOB_GASDECOMP=1` over the 64-module corpus; `full − z_<class>` is each class's exact executed gas). Gas is **independent of recompiler lowering** — inline, trampoline, and interpreter charge identically (empirically 1187 for ERC20 in all three), so this is `ext` vs `ref`, not "inline vs ref".

| component | interim (16/6) | **now (4/4)** |
|---|--:|--:|
| ref total | 20,261 | 20,261 |
| vec total | 23,865 (1.178×) | **21,695 (1.071×)** |
| — vec scalar-only | 16,041 (−4,220) | 16,041 (−4,220) |
| — wide **memory** (`wld`/`wst`) | +3,810 | **+2,540** |
| — wide **convert** (`wzext`/`wtrunc`) | +2,814 | **+2,814** |
| — wide linear | +1,200 | **+300** |
| **net (vec − ref)** | +3,604 | **+1,434** |

After the recalibration the residual is **convert-dominated** (`wzext`/`wtrunc`, +2,814) — this deploy-path corpus promotes narrow constants to i256 and truncates back, extra ops with no scalar counterpart, correctly priced at 2 each and removable only by narrowing (§9), not cost-tuning. Memory drops to +2,540 (now ~neutral per op) and linear to +300. So the earlier "converts" intuition was right about the *residual* — it just took removing the memory/linear miscalibration to expose it. **On real contracts converts are ~0 and memory is neutral, so `ext` gas is now 0.993× ref** (§12).

## 7. Wide-instruction usage and width

**12,678 wide instructions** over 98 modules. Width is **99.9% i256** (4,842 `i256` / 4 `i512` in linked blobs; no i128/i1024).

| instruction | count | share |
|---|--:|--:|
| `wld` | 2,372 | 18.7% |
| `wzext` | 2,363 | 18.6% |
| `wst` | 1,682 | 13.3% |
| `wtrunc` | 1,621 | 12.8% |
| `wsrl` | 1,504 | 11.9% |
| `wseq` | 752 | 5.9% |
| `wsll` | 401 | 3.2% |
| `wor` | 356 | 2.8% |
| `wsltu` | 333 | 2.6% |
| `wbswap` | 306 | 2.4% |
| `wadd` | 303 | 2.4% |
| `wand` | 244 | 1.9% |
| `wmv2r` | 147 | 1.2% |
| `wsext` | 87 | 0.7% |
| `wminu` | 43 | 0.3% |
| `wsne` | 38 | 0.3% |
| `wxor` | 26 | 0.2% |
| `wslt` | 26 | 0.2% |
| `wsub` | 24 | 0.2% |
| `wsignextend` | 12 | 0.1% |
| `wdivu` | 9 | 0.1% |
| `wmul` | 8 | 0.1% |
| `wdiv` | 7 | 0.1% |
| `wremu` | 5 | 0.0% |
| `wrem` | 4 | 0.0% |
| `wmv4r` | 2 | 0.0% |
| `wsra` | 1 | 0.0% |
| `wmulmod` | 1 | 0.0% |
| `wexp` | 1 | 0.0% |

## 8. Width mechanism (vtype vs funct7) and the machine outliner

Object `.text` over the extension arms (98 modules): funct7 **259,914**, vsetvli-MO **275,384**, vsetvli-noMO **280,172**. funct7 is smaller at the object level (no `vsetivli`), but the linker drops the `vsetivli`s so at blob level vsetvli-noMO wins (§6a) and keeps `custom-1/3` free.

**Machine outliner:** allowing it makes the blob **+6.1%** larger (larger-or-equal on every contract, see §6a MO vs noMO), compile ~6% slower, with identical runtime. **Disable it.**

## 9. NewYork IR — type inference impact

The NewYork pipeline is not yet fast enough to benchmark at scale: `resolc --newyork` exceeds a 60 s cap on 64/83 contracts (vs 1–3 s on the Yul path), and `llc` compiles only 37/99. Type inference is **load-bearing** — disabling it first produced degenerate code (un-inferred values read the lattice's `I1` bottom as a width); fixed to default to `i256`.

Valid impact of **disabling type inference** (34 contracts that ran in both). `gas`/`interp` are noisy where a contract hits the 2M gas cap.

| benchmark | blob withTI | blob noTI | Δblob | .text noTI | interp withTI/noTI (µs) | recomp withTI/noTI (µs) |
|---|--:|--:|--:|--:|--:|--:|
| Uint128Arithmetic.sol.Uint128Arithmetic | 3,508 | 1,962 | -44.1% | 1,702 | 3,683/24,116 | 2,861/2,764 |
| ExtCode.sol.ExtCode | 1,863 | 1,868 | +0.3% | 1,854 | 84/85 | 81/78 |
| CustomErrorArgs.sol.CustomErrorArgs | 1,792 | 1,792 | +0.0% | 1,792 | 43/43 | 42/42 |
| CallGas.sol.Other | 1,565 | 1,565 | +0.0% | 1,480 | 70/81 | 57/52 |
| Events.sol.Events | 1,532 | 1,532 | +0.0% | 1,368 | 4,342/4,342 | 7,558/7,569 |
| LinkerI32BoundaryFoldBug.sol.LinkerI32Bo | 1,397 | 1,397 | +0.0% | 1,374 | 4,089/4,342 | 7,548/7,552 |
| UlongRem.sol.UlongRem | 1,384 | 1,384 | +0.0% | 1,352 | 4,366/4,343 | 7,562/7,558 |
| Transaction.sol.TransactionOrigin | 1,358 | 1,358 | +0.0% | 1,384 | 4,807/4,866 | 7,484/7,477 |
| Send.sol.Send | 1,260 | 1,352 | +7.3% | 1,440 | 26/33 | 50/33 |
| CopyOverlapBug.sol.CopyOverlapBug | 1,303 | 1,303 | +0.0% | 1,396 | 4,273/4,366 | 7,565/7,558 |
| Context.sol.Context | 1,222 | 1,222 | +0.0% | 1,300 | 4,583/4,762 | 7,472/7,483 |
| UnalignedMStore8Bug.sol.UnalignedMStore8 | 1,202 | 1,202 | +0.0% | 1,210 | 5,722/5,724 | 6,992/7,001 |
| SubUnderflowZext.sol.SubUnderflowZext | 1,185 | 1,185 | +0.0% | 1,268 | 4,342/4,342 | 7,560/7,575 |
| UnalignedMloadNativeBug.sol.UnalignedMlo | 1,179 | 1,179 | +0.0% | 1,276 | 4,367/4,365 | 7,568/7,554 |
| UnalignedMStoreBug.sol.UnalignedMStoreBu | 1,170 | 1,170 | +0.0% | 1,150 | 16,161/16,411 | 1,290/1,290 |
| PanicInterveneBug.sol.PanicInterveneBug | 1,155 | 1,155 | +0.0% | 1,162 | 108/70 | 61/60 |
| MStore8.sol.MStore8 | 1,151 | 1,151 | +0.0% | 1,296 | 4,344/4,342 | 7,556/7,569 |
| Fibonacci.sol.FibonacciIterative | 1,144 | 1,144 | +0.0% | 1,248 | 4,358/4,343 | 7,563/7,565 |
| Computation.sol.Computation | 1,137 | 1,137 | +0.0% | 1,340 | 4,486/4,342 | 7,567/7,570 |
| SAR.sol.SAR | 1,110 | 1,110 | +0.0% | 1,182 | 4,365/4,390 | 7,740/7,725 |
| FmpNativeStoreBug.sol.FmpNativeStoreBug | 1,271 | 1,092 | -14.1% | 1,164 | 2,849/4,586 | 2,569/7,329 |
| Value.sol.ValueTester | 1,087 | 1,087 | +0.0% | 1,114 | 4,359/4,353 | 7,751/7,751 |
| ConstReturnOverflowBug.sol.ConstReturnOv | 1,024 | 1,024 | +0.0% | 1,050 | 43/43 | 56/55 |
| PanicCodeBug.sol.PanicCodeBug | 958 | 958 | +0.0% | 1,042 | 80/54 | 56/38 |
| Baseline.sol.Baseline | 940 | 940 | +0.0% | 1,014 | 42/42 | 34/34 |
| Coinbase.sol.Coinbase | 856 | 856 | +0.0% | 980 | 58/30 | 51/50 |
| Selfdestruct.sol.SelfdestructTester | 843 | 843 | +0.0% | 928 | 52/52 | 38/48 |
| GasLeft.sol.GasLeft | 811 | 811 | +0.0% | 904 | 30/51 | 50/34 |
| BaseFee.sol.BaseFee | 805 | 805 | +0.0% | 938 | 44/50 | 43/33 |
| GasPrice.sol.GasPrice | 792 | 792 | +0.0% | 896 | 45/45 | 51/34 |
| GasLimit.sol.GasLimit | 790 | 790 | +0.0% | 890 | 30/44 | 50/35 |
| Create.sol.CreateA | 592 | 592 | +0.0% | 812 | 43/43 | 49/50 |
| Create2.sol.CreateA | 592 | 592 | +0.0% | 812 | 42/43 | 50/49 |
| Balance.sol.BalanceReceiver | 538 | 538 | +0.0% | 804 | 41/28 | 41/49 |
| **TOTAL (34)** | **40,516** | **38,888** | **-4.0%** | **40,922** | **86,377/109,172** | **113,066/117,664** |

Most contracts are unchanged (inference finds nothing to narrow); the effect concentrates in arithmetic ones (`Uint128Arithmetic`: −44% blob, ~6.5× slower interp). **Type inference trades a little code size for speed** — it narrows `i256`→`i64`/`i128`, producing larger but faster code, because scalar ops avoid the expensive wide path. *(This table isolates inference by comparing TI-on vs TI-off within the NewYork pipeline.)*

### Full benchmark re-measurement on the NewYork corpus (type inference ON) vs the Yul baseline

The complementary question to the table above: the whole pipeline — **NewYork IR (TI on) vs the Yul-path IR** of §5–§8. The ref/vec/w harness was re-run on `ir-corpus-newyork` (Yul baseline `per-bench-wreg-yul.tsv`, NewYork `per-bench-wreg-newyork.tsv`, diff `compare_newyork.py`).

**Coverage first — the NewYork pipeline is still maturing (see the §9 head).** Its IR is fine for the base ISA but frequently crashes or times out `llc` *once the wide extension is on*:

| corpus | ref (compiled / ran) | vec | w |
|---|--:|--:|--:|
| Yul path | 96 / 67 | 98 / 80 | 99 / 84 |
| **NewYork (TI on)** | 96 / 68 | **37 / 36** | **51 / 51** |

`ref` (no extension) is unaffected; the extension arms collapse to a minority of the corpus (`llc` core-dumps / 60 s timeouts on the rest). Note the **`w` path compiles a strict superset of `vec`** — 51 vs 37 modules (14 that `vec` crashes on, 0 the reverse) — because it avoids the fragile RVV/`vtype` codegen (reinforces §11's "cleaner integration" finding).

**Where it does compile, type inference is a large win.** On the 36 modules that compile *and* run on the `vec` arm in **both** corpora (so this isolates the IR change, not the module set):

| metric (vec arm, 36 shared modules) | Yul | NewYork (TI on) | NewYork ÷ Yul |
|---|--:|--:|--:|
| `.text` (object, bytes) | 69,668 | 45,272 | **0.65×** |
| blob (shipped, bytes) | 64,167 | 43,699 | **0.68×** |
| **gas** (deterministic) | 11,920 | 3,620 | **0.30×** |
| interpreter (µs, amortized) | 80.7 | 31.5 | **0.39×** |
| recompiler (µs, amortized) | 104.3 | 103.5 | 0.99× |

Narrowing `i256`→`i64`/`i128` leaves far fewer wide ops, so the win lands on **gas — down 3.3×** (−70%), interpreter −61%, code size −34%; the **recompiler is at parity** (0.99×, already fast on wide ops per §5/§6c). This is the opposite of the TI-on-vs-off table (which showed TI larger): against the **Yul** baseline the whole NewYork pipeline is both smaller and cheaper. (These NewYork gas numbers were measured before the §6d `WIDE_MEMORY/LINEAR` recalibration, so both the Yul and NewYork absolute gas would now be lower; the NewYork-vs-Yul *ratio* holds since both shift together.) w-vs-vec stays at recompiler parity (indicative — only 31–36 modules).

**Bottom line.** Where the NewYork front-end compiles, benchmarks are dramatically cheaper (**~3.3× less gas, ~2.6× faster interp, ~1.5× smaller**) at recompiler parity, and `w` compiles more of them than `vec`. The blocker is `llc` robustness (only 37/99 `vec` compile) — a "where it compiles" result, not a full-corpus replacement for §6 (the §10 NewYork item).

## 10. Status and next

- Working end to end on both backends; full polkavm suite passes.
- **Inline cheap wide compute in the recompiler — DONE, default.** `add`/`sub`/`and`/`or`/`xor`, `slt_u/s`, `seq`/`sne`, `trunc`/`zext`/`move` emit inline native (only `rcx`); heavy, shift/`min`/`max`/`bswap`/`sext`, and i512+ stay on the trampoline. Needed the 69→96 B cap raise. Effect: **1.00× ref, −23% vs trampoline** (§6c); ERC20 regression erased. Kill switch `POLKAVM_DISABLE_WIDE_INLINE`. Open: shifts/`min`/`max`/`bswap`/`sext` need assembler primitives; i512+ needs a further cap raise.
- **i512+ wide load/store fixed — DONE.** The >i256 copy loop advanced pointers with a 32-bit `add`, truncating the >4 GiB wide-file pointer and faulting; now 64-bit, with regression coverage (only i256 was previously exercised).
- **Machine outliner disabled** (§8); **gas recalibrated** (§6d): dropping the marshal term (+2.47×→1.18×) then capping linear/memory ops at N×64-bit (→1.07× toy, **0.993× real**); residual is convert ops the §7/§9 narrowing would remove.
- **Open bug:** a wide instruction right after a call has no dataflow-supplied width (3 XENCrypto modules).
- **NewYork IR** needs a faster Yul→IR stage before scale benchmarking (§9).

*Harnesses/data in `benchmarks/analysis/vsetivli-tradeoff/`: `wide_microbench.rs`, `measure_per_bench.py`, `compare_ti.py`, `per-bench-default.tsv`, `ti-compare-results.txt`, `usage-default-counts.txt`, `microbench-{interp,recomp}.txt`.*

## 11. Experiment: a dedicated register file instead of RVV (XReviveW)

The shipped extension makes two coupled choices: it **reuses the RVV register groups** (`v0–v31`) for i256/i512/i1024, and **takes the width from `vtype`** via a linker-consumed `vsetivli` (§8). This experiment separates the two axes.

**Two axes, three design points.** *Register file:* reuse RVV (forces RVV-avoidance, LMUL2 even alignment, and the outliner-vs-`vtype` restriction of §8) vs. a dedicated file. *Width mechanism* (custom-2 carries no width): (a) `vtype` set by `vsetivli` (**shipped `vec`**); (b) a dedicated **`revive.set_width`** mode instruction (below); (c) **no mechanism** — fixed, implicit width (**this prototype, `w`**).

**What was built (`+xrevivew`).** A dedicated 16-entry file `W0–W15`, i256 legal, **fixed 256-bit width, no width instruction** (no `vsetivli`, no `set_width`) — the linker defaults any unresolved wide op to 256 bits (`POLKAVM_ASSUME_W256`), dropping all RVV coupling. Needed an LLVM register class + CC + ISel + copy/spill and one linker flag; held to i256. `W0–W15` encode as `0,2,…,30` so the linker's existing VRM2-field decode reads them unchanged.

### Calling convention

i256 args/results pass in **`W0–W7`** (eight arg/return registers), spilling to 32-byte-aligned 32-byte slots when exhausted (`RISCVCallingConv.cpp`) — mirroring `vec`'s `VRM2` sequence, applied in both `CC_RISCV` and revive's `CC_RISCV_FastCC` (the FastCC path is used by resolc's internal functions; omitting it there was an early crash). `W8–W15` are scratch; the file isn't callee-saved, so an i256 live across a call spills via `wst`/`wld` (`spill_i256` lit test). With fixed width there is **no width state to preserve across a call** — exactly the bug class `vec` still carries (§10).

### The `set_width` instruction: definition and why it is *not* here

The originally-proposed variant replaced `vsetivli`/`vtype` with a dedicated mode instruction **`revive.set_width <bytes>`** (up to 64 → i512, extensible to i1024), setting the width every following wide op inherits until the next `set_width` — a vector-like CC for who owns the mode, plus a **`set_width`-elimination pass** (LLVM VL/VType-optimizer analogue) to drop redundant re-sets. It is the width-axis mid-point: variable width like `vtype`, but a first-class extension instruction.

**The prototype omits it** because §7 measured width at **99.9% i256** (4,842 `i256` vs 4 `i512`; no i128/i1024), so variable width buys almost nothing while costing: an extra instruction per width change (needing the elimination pass to remove); that pass plus the interprocedural width dataflow the linker runs for `vec` (`resolve_wide_widths`, §8) — machinery a fixed width deletes; mode state across calls (reintroducing the §10 "no width after a call" bug fixed width can't have); and recompiler/linker mode tracking (why `vtype` complicates the outliner, §8). So `set_width` pays for generality the corpus doesn't use — which is why the prototype is **i256-only** (i512/i1024 would need a width mechanism back). For an all-i256 corpus, option (c) — no mechanism — dominates (a) and (b).

### Results (80 modules that link in both `vec` and `w`)

Deterministic metrics as totals over the 80-module set; `ref` (base ISA, no extension) as the baseline, over the subset that also links in `ref` (n noted). `w ÷ vec` is the head-to-head — both use the extension, so it is the clean comparison.

| metric | ref (base ISA) | vec (RVV + `vtype`) | **w (dedicated file)** | w ÷ vec |
|---|--:|--:|--:|--:|
| `.text` (object) | 250,378 *(n=77)* | 206,280 | **191,624** | **0.929×** |
| blob (shipped) | 177,773 *(n=64)* | 196,916 | **192,761** | **0.980×** |
| gas (deterministic) | 20,261 *(n=64)* | 29,423 | **28,729** | **0.976×** |

*(The `vec`/`w` gas totals predate the §6d `WIDE_MEMORY/LINEAR` recalibration; both arms use the same costs so the **w÷vec 0.976×** ratio — the point of this experiment — is unaffected, only the absolute totals would now be lower.)*

Wall-time as the **per-module median ratio** (aggregate totals are overhead-noise-dominated per §6c, so the median is the reliable measure; interpreter over all 80, recompiler/`ref` over the 64 that link in all arms):

| backend | w ÷ vec | w ÷ ref | vec ÷ ref |
|---|--:|--:|--:|
| interpreter | **0.998×** | 1.180× | 1.144× |
| **recompiler** (production path) | **1.000×** | 1.002× | 0.997× |

**Findings.** The **recompiler — the production execution path — is at parity** (median w÷vec 1.000×, and w÷ref 1.002× ≈ base ISA), confirming the core prediction: the width mechanism is linker-consumed and never reaches the blob, so `vec` and `w` ship the same wide-op stream and execute identically. The interpreter is likewise at parity between `vec` and `w` (0.998×). Where `w` differs from `vec` it is *smaller*: object `.text` **−7%** (the `vsetivli` are simply gone, and no RVV-alignment padding), shipped blob **−2%** (smaller in 74 of 80 modules), gas **−2.4%** (marginally fewer instructions). So dropping the RVV coupling and the width mechanism is at parity on execution and modestly ahead on size — never worse — for this all-i256 corpus.

**Caveats.** The `w` path is a lean prototype vs a production-tuned `vec`, so the ~2% blob/gas edge is partly codegen happenstance — the durable wins are the ~7% object-code reduction and the cleaner integration (no RVV coupling), not shipped execution (recompiler is equal). It is **i256-only** by design (see `set_width` above). Two review-found correctness bugs were fixed (they affect computed values only — which `runblob` doesn't check — not the size/gas numbers above): (1) i256 `div`/`rem` was software-expanded because `setMaxDivRemBitWidthSupported` stayed at the default 128 for the W-path, so `ExpandLargeDivRem` rewrote division before ISel — raised to 256; (2) the i256 register move was `wmv1r` (128-bit), dropping each copy's high half — corrected to `wmv2r`. Branches `kvpanch/wreg_prototype`; data `per-bench-wreg.tsv`; tests `llvm/test/CodeGen/RISCV/xrevivew.ll`.

**Value-correctness (differential).** Since gas and size can't see a value miscompile, `diffcheck.py` + `runblob RUNBLOB_TRACE=1` compares each contract's *observable values* (host-call sequence, storage writes as key→value, return payload — all layout-independent) across ref/vec/w: **`w` is value-identical to `vec` on all 80 modules** (0 mismatches). Coverage caveat — empty calldata and stubbed host calls exercise only the deploy path and sink-reaching values, so the `wmv1r`/`wmv2r` bug produces **zero** w-vs-vec diffs here (values reach storage via `wld`/`wst`, not the move); the lit tests, not this harness, guard register-level bugs. It did catch real ext-vs-`ref` divergences (a host-stub/byte-order confound), confirming detection works.

## 12. Real production contracts (top mainnet contracts by usage)

§5–§11 use revive's integration corpus (toy contracts). This section re-runs ref/vec/w on **real, most-used Ethereum-mainnet contracts**, fetched verified from Sourcify and compiled with `resolc` (`fetch_compile.py`). None overlap the toy corpus, so nothing is skipped for duplication.

**Hard toolchain gate — `resolc` supports only solc 0.8.0–0.8.36.** Exactly the contracts most people mean by "most-used" are pre-0.8 and **cannot be compiled by this toolchain at all**. The canonical top set:

| rank-ish (by usage) | contract | solc | in scope? |
|---|---|---|--:|
| USDT | TetherToken | 0.4.18 | ❌ pre-0.8 |
| USDC | FiatTokenV2_2 | 0.6.12 | ❌ pre-0.8 |
| WETH9 | WETH9 | 0.4.19 | ❌ pre-0.8 |
| Uniswap V2 Router | UniswapV2Router02 | 0.6.6 | ❌ pre-0.8 |
| Uniswap V3 Router | SwapRouter02 | 0.7.6 | ❌ pre-0.8 |
| DAI | Dai | 0.5.12 | ❌ pre-0.8 |
| Multicall3 | Multicall3 | 0.8.12 | ✅ |
| Permit2 | Permit2 | 0.8.17 | ✅ |
| Seaport 1.6 | Seaport | 0.8.24 | ⚠️ compiles-fail (below) |
| ERC-4337 EntryPoint | EntryPoint | 0.8.17 / 0.8.23 | ✅ compiles |
| Uniswap Universal Router | UniversalRouter | 0.8.26 | ✅ |
| Uniswap V4 PoolManager | PoolManager | 0.8.26 | ✅ compiles |

So the measured set is the **most-used 0.8.x contracts** (canonical 0.8.x ones + high-usage fill-ups to reach ten). What each does:

| contract | what it does |
|---|---|
| **Multicall3** | Batches many read-only calls into one, aggregating results — used by nearly every dapp frontend/indexer to read chain state in a single RPC round-trip. |
| **Permit2** (Uniswap) | Universal token-approval layer: EIP-712 signature-based approvals and transfers, so a user approves once and any integrated protocol can pull tokens. |
| **EntryPoint v0.6 / v0.7** (ERC-4337) | Account-abstraction singleton: validates and executes bundles of UserOperations (smart-wallet txs), handling gas prepayment and refunds. |
| **Universal Router** (Uniswap) | Single router that composes V2/V3/V4 swaps and NFT buys in one call by decoding a command stream. |
| **Uniswap V4 PoolManager** | The singleton holding all V4 pools; manages liquidity, swaps, and hook callbacks with flash accounting. |
| **1inch AggregationRouterV6** | DEX aggregator router that splits/routes a trade across many liquidity sources for best execution. |
| **Morpho Blue** | Minimal immutable lending primitive: isolated markets (one collateral, one loan asset) with supply/borrow/liquidate and interest accrual. |
| **Ethena USDe** | Synthetic-dollar stablecoin (ERC20) backed by delta-hedged collateral; mint/redeem + transfers. |
| **Ethena sUSDe** (StakedUSDeV2) | Yield-bearing staking vault (ERC-4626) for USDe: deposit, accrue yield, cooldown-based withdrawal. |

(For context, the pre-0.8 contracts that can't be compiled: **USDT/USDC/DAI** are stablecoin ERC20 tokens; **WETH9** wraps ETH as an ERC20; **Uniswap V2/V3 routers** are AMM swap routers.)

All are heavy wide-integer users — unlike the toy corpus, every one carries thousands of `i256` IR ops (Multicall3 974 → 1inch 8,423), i.e. real production code genuinely exercises the extension.

**Coverage — the largest real contracts stress the extension toolchain.** Of the 10, the full compile→link→run pipeline completes for far fewer than on the toy corpus:

| outcome | modules |
|---|---|
| ran in all three arms (ref+vec+w) | **4** — Multicall3, Permit2, 1inch V6, Ethena USDe |
| ran in vec+w (ref link-failed) | +1 — Morpho Blue |
| ran in w only | +1 — Universal Router (ref `llc`-fail, vec link-fail) |
| `llc` compile-fail (all arms) | Uniswap V4 PoolManager, Ethena sUSDe |
| `polkatool` link-fail (all arms) | EntryPoint v0.6, EntryPoint v0.7 |

Two honest observations: (1) large real contracts hit `llc` crashes and linker limits the toy corpus never exercised — a robustness gap, not a perf result; (2) the **`w` arm is again the most robust** — it is the *only* arm that completes Universal Router (ref won't compile, vec won't link), echoing §9/§11.

**Results (the 4 contracts measurable in all three arms; deterministic metrics as totals).** ref = base ISA, no extension.

| metric | ref | vec | w | vec ÷ ref | w ÷ ref |
|---|--:|--:|--:|--:|--:|
| `.text` (object) | 695,570 | 660,210 | 630,598 | 0.949× | **0.907×** |
| **blob (shipped)** | 744,110 | 540,187 | 527,025 | **0.726×** | **0.708×** |
| gas (deterministic) | 3,180 | 3,157 | 3,136 | **0.993×** | **0.986×** |
| interpreter (µs) | 16.2 | 17.0 | 18.9 | 1.05× | 1.17× |
| recompiler (µs) | 12.8 | 14.4 | 14.4 | 1.12× | 1.12× |

**Per-benchmark code size.** Object `.text` is available for every contract that compiled (8), the shipped blob only for those that also linked (5). The two tell different stories: `.text` is roughly *flat* vs ref (the `vsetivli` + wide ops can even make it slightly larger), but the shipped **blob shrinks ~27%** because the linker consumes the `vsetivli`, one wide op replaces a whole scalar limb-chain, and dead code is stripped.

*Object `.text` (bytes):*

| contract | ref | vec | w | vec/ref | w/ref | w/vec |
|---|--:|--:|--:|--:|--:|--:|
| Multicall3 | 44,448 | 39,274 | 37,304 | 0.884× | 0.839× | 0.950× |
| Permit2 | 141,150 | 145,734 | 141,310 | 1.032× | 1.001× | 0.970× |
| Ethena USDe | 91,096 | 85,704 | 81,982 | 0.941× | 0.900× | 0.957× |
| Morpho Blue | 219,534 | 207,468 | 197,792 | 0.945× | 0.901× | 0.953× |
| 1inch V6 | 418,876 | 389,498 | 370,002 | 0.930× | 0.883× | 0.950× |
| Universal Router | — | 175,526 | 166,914 | — | — | 0.951× |
| EntryPoint v0.6 | 301,506 | 321,150 | 312,230 | 1.065× | 1.036× | 0.972× |
| EntryPoint v0.7 | 224,288 | 224,556 | 219,034 | 1.001× | 0.977× | 0.975× |
| **TOTAL (n=7 all arms)** | **1,440,898** | **1,413,384** | **1,359,654** | **0.981×** | **0.944×** | **0.962×** |

*Shipped blob (bytes) — the headline metric:*

| contract | ref | vec | w | vec/ref | w/ref | w/vec |
|---|--:|--:|--:|--:|--:|--:|
| Multicall3 | 48,805 | 32,441 | 31,501 | 0.665× | 0.645× | 0.971× |
| Permit2 | 149,678 | 112,356 | 110,291 | 0.751× | 0.737× | 0.982× |
| Ethena USDe | 94,738 | 69,002 | 67,293 | 0.728× | 0.710× | 0.975× |
| Morpho Blue | — | 168,513 | 163,616 | — | — | 0.971× |
| 1inch V6 | 450,889 | 326,388 | 317,940 | 0.724× | 0.705× | 0.974× |
| Universal Router | — | — | 140,323 | — | — | — |
| **TOTAL (n=4 all arms)** | **744,110** | **540,187** | **527,025** | **0.726×** | **0.708×** | **0.976×** |

**Per-benchmark wall-time (µs, amortized best-of).** Blanks are arms that didn't complete. Treat these as indicative (n is small and per-call overhead dominates, §6c) — the deterministic blob/gas above are the reliable signal.

*Interpreter:*

| contract | ref | vec | w | vec/ref | w/ref | w/vec |
|---|--:|--:|--:|--:|--:|--:|
| Multicall3 | 1.42 | 1.01 | 1.05 | 0.71× | 0.74× | 1.04× |
| Permit2 | 3.52 | 2.54 | 3.79 | 0.72× | 1.07× | 1.49× |
| Ethena USDe | 2.16 | 1.49 | 2.24 | 0.69× | 1.04× | 1.50× |
| Morpho Blue | — | 5.25 | 5.25 | — | — | 1.00× |
| 1inch V6 | 9.09 | 11.94 | 11.85 | 1.31× | 1.30× | 0.99× |
| Universal Router | — | — | 2.32 | — | — | — |

*Recompiler (production path):*

| contract | ref | vec | w | vec/ref | w/ref | w/vec |
|---|--:|--:|--:|--:|--:|--:|
| Multicall3 | 3.14 | 3.19 | 3.19 | 1.02× | 1.02× | 1.00× |
| Permit2 | 4.12 | 4.08 | 4.03 | 0.99× | 0.98× | 0.99× |
| Ethena USDe | 3.21 | 3.21 | 3.23 | 1.00× | 1.01× | 1.01× |
| Morpho Blue | — | 3.43 | 3.07 | — | — | 0.90× |
| 1inch V6 | 2.38 | 3.96 | 3.92 | 1.66× | 1.65× | 0.99× |
| Universal Router | — | — | 3.50 | — | — | — |

Two patterns stand out. (1) On the small/mid contracts the **recompiler is at parity** (Multicall3/Permit2/Ethena all 0.98–1.02× ref), while the **interpreter is notably *faster* with `vec`** (0.69–0.72× ref — one wide op replaces a whole scalar limb chain). (2) The **1inch outlier goes the other way** — recompiler 1.66× and interpreter 1.31× ref — because it is by far the most wide-op-dense contract (8,423 i256 IR lines, 3.3 MB): its many *un-inlined* wide ops (`mul`/shift/`div`/`mod`) each pay the ~50 ns `syscall_wide` trampoline crossing (§5), which is exactly the case the §10 "inline more wide ops" work targets. `w` tracks `vec` on the recompiler (parity, and 0.90× on Morpho Blue) but is slower than `vec` in the *interpreter* on Permit2/Ethena (w/vec ~1.5×) — a real but interpreter-only, small-n effect.

**Findings — the extension helps *more* on real code than on the toy corpus.**
- **Shipped blob shrinks ~27% with the extension** (vec/ref 0.73×, w/ref 0.71×), a bigger win than the ~20% on the toy corpus (§6a). Real contracts carry far more i256 limb-chain arithmetic, and one wide op replaces a whole chain — so the more real the code, the more the extension removes.
- **Gas is now ≈neutral: vec/ref 0.993×** (below the base ISA), after recalibrating `WIDE_MEMORY 6→4` and `WIDE_LINEAR 16→4` (§6d). Before the recalibration it was +4.8%; the toy corpus similarly went +18% → +7%. Real contracts spend gas on genuine wide arithmetic that deletes scalar limb-chains, so once memory/linear are priced at the scalar cost the extension breaks even or wins.
- **`w` stays smaller than `vec`** (object 0.907× vs 0.949× of ref; blob 0.708× vs 0.726×) and at recompiler parity — consistent with §11 on real code.
- Wall-time (interp/recomp) is noisy at n=4 and dominated by the 3.3 MB 1inch module (per-call overhead per §6c); treat the deterministic blob/gas as the reliable signal.
- **The blocker is toolchain robustness on large contracts**, not the extension's value: closing the `llc`/linker gaps (and the pre-0.8 solc gap) would let the remaining blue-chips be measured.

### Gas is ≈neutral (0.993×) — decomposed, before and after the recalibration

`runblob RUNBLOB_GASDECOMP=1` re-runs each blob under cost models that zero one wide-op class at a time; since gas metering never changes control flow, `full − z_<class>` is exactly that class's executed gas. Gas is **independent of recompiler lowering** (inline/trampoline/interpreter charge identically). Per contract, before (`WIDE_MEMORY=6`, `WIDE_LINEAR=16`) and after (`4`/`4`) the recalibration:

| contract | ref | vec (was 6/16) | vec (now 4/4) | now ÷ ref |
|---|--:|--:|--:|--:|
| Multicall3 | 181 | 229 (+27%) | 201 | 1.11× |
| Ethena USDe | 324 | 358 (+11%) | 332 | 1.02× |
| Permit2 | 557 | 620 (+11%) | 558 | 1.00× |
| 1inch V6 | 2,118 | 2,126 (+0.4%) | 2,066 | **0.98×** |
| **total (4)** | **3,180** | **3,333 (1.048×)** | **3,157 (0.993×)** | **0.993×** |

Aggregate wide-op gas now decomposes to **memory +208, convert +4, linear +24**, offset by **−259** scalar (wide ops delete scalar limb-chains) → **net −23** (0.993×). Findings:

- **After the fix the extension is gas-neutral-to-favorable.** The dominant driver was wide `load`/`store` priced at 6 vs the ~4-instruction scalar 4-limb access; at 4 that gap closes. 1inch (arithmetic-heavy) is now *below* ref (0.98×) because its wide ops delete more scalar work than they add; Multicall3 (small values shuffled through 256-bit slots, little real wide math) is the only one still >1 (1.11×).
- **Residual is tiny and memory-shaped, removed by narrowing.** The remaining per-op memory cost reflects that a wide `wld`/`wst` still moves a full 256-bit word even for a 64-bit value; type inference (§9 NewYork) narrows `i256`→`i64` and turns it back into a scalar load — why NewYork cut gas 3.3× (§9).
- **Converts are ~0 on real contracts** (unlike the toy deploy corpus, §6d) — real measured paths do genuine arithmetic, not constructor-time constant promotion.

*Corpus in `benchmarks/ir-corpus-real/` (10 verified-mainnet contracts, compiled from Sourcify sources); harness `measure_wreg.py`; data `per-bench-wreg-real.tsv`; fetch/compile driver `fetch_compile.py`; per-contract compile status `real-compile-status.tsv`; gas decomposition via `runblob RUNBLOB_GASDECOMP=1`.*
