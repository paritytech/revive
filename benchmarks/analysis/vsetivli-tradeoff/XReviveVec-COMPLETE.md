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

### 256-bit

| op | interp ext | recomp ext | interp ref (scalar) | recomp ref (scalar) | interp ext vs ref | recomp ÷ interp (ext) |
|---|--:|--:|--:|--:|--:|--:|
| `add` | 20.0 | 57.9 | 317.9 | 7.0 | 15.9× | 2.9× |
| `sub` | 20.1 | 58.3 | 306.6 | 7.1 | 15.3× | 2.9× |
| `mul` | 36.5 | 56.8 | 1,016.6 | 22.4 | 27.9× | 1.6× |
| `and` | 20.4 | 54.4 | 145.8 | 2.5 | 7.1× | 2.7× |
| `or` | 20.2 | 54.4 | 144.7 | 2.5 | 7.2× | 2.7× |
| `xor` | 20.7 | 54.5 | 145.0 | 2.5 | 7.0× | 2.6× |
| `shl` | 23.9 | 59.8 | 202.7 | 3.4 | 8.5× | 2.5× |
| `shr_l` | 24.4 | 57.4 | — | — | — | 2.4× |
| `shr_a` | 24.5 | 55.9 | — | — | — | 2.3× |
| `slt_u` | 16.4 | 40.1 | 405.1 | 9.9 | 24.7× | 2.4× |
| `slt_s` | 17.2 | 40.6 | — | — | — | 2.4× |
| `seq` | 16.4 | 39.8 | 234.0 | 3.4 | 14.2× | 2.4× |
| `sne` | 16.4 | 39.8 | — | — | — | 2.4× |
| `min_u` | 18.1 | 58.2 | — | — | — | 3.2× |
| `min_s` | 18.2 | 55.8 | — | — | — | 3.1× |
| `max_u` | 17.8 | 58.2 | — | — | — | 3.3× |
| `max_s` | 17.8 | 55.8 | — | — | — | 3.1× |
| `move` | 14.8 | 53.8 | 70.1 | 1.2 | 4.7× | 3.6× |
| `bswap` | 17.5 | 52.5 | 102.6 | 1.9 | 5.9× | 3.0× |
| `zext` | 15.1 | 54.4 | 47.0 | 1.1 | 3.1× | 3.6× |
| `sext_w` | 12.4 | 52.5 | — | — | — | 4.2× |
| `trunc` | 10.8 | 38.8 | 17.4 | 0.2 | 1.6× | 3.6× |
| `signext` | 23.2 | 53.7 | — | — | — | 2.3× |
| `load` | 28.3 | 3.0 | — | — | — | 0.1× |
| `store` | 16.8 | 1.9 | — | — | — | 0.1× |
| `div_u` | 2,782.2 | 4,497.9 | — | — | — | 1.6× |
| `div_s` | 2,798.3 | 4,553.4 | — | — | — | 1.6× |
| `rem_u` | 2,783.4 | 4,511.1 | — | — | — | 1.6× |
| `rem_s` | 2,803.2 | 4,536.7 | — | — | — | 1.6× |
| `addmod` | 5,571.6 | 8,996.8 | — | — | — | 1.6× |
| `exp` | 5,550.1 | 10,183.2 | — | — | — | 1.8× |
| `mulmod` | 6,788.0 | 10,782.8 | — | — | — | 1.6× |

### 128-bit

| op | interp ext | recomp ext | interp ref (scalar) | recomp ref (scalar) | interp ext vs ref | recomp ÷ interp (ext) |
|---|--:|--:|--:|--:|--:|--:|
| `add` | 19.0 | 54.3 | 173.6 | 3.5 | 9.1× | 2.9× |
| `sub` | 18.8 | 55.0 | 167.8 | 4.0 | 8.9× | 2.9× |
| `mul` | 24.5 | 58.2 | 288.9 | 5.2 | 11.8× | 2.4× |
| `and` | 18.1 | 47.8 | 72.7 | 1.1 | 4.0× | 2.6× |
| `or` | 19.1 | 47.7 | 72.0 | 1.1 | 3.8× | 2.5× |
| `xor` | 18.8 | 47.4 | 72.1 | 1.1 | 3.8× | 2.5× |
| `shl` | 21.4 | 61.2 | 78.1 | 1.1 | 3.6× | 2.9× |
| `shr_l` | 21.3 | 61.2 | — | — | — | 2.9× |
| `shr_a` | 21.3 | 59.2 | — | — | — | 2.8× |
| `slt_u` | 15.9 | 39.1 | 199.2 | 4.3 | 12.5× | 2.5× |
| `slt_s` | 16.3 | 40.2 | — | — | — | 2.5× |
| `seq` | 15.8 | 39.4 | 119.3 | 2.1 | 7.5× | 2.5× |
| `sne` | 16.1 | 39.4 | — | — | — | 2.4× |
| `min_u` | 18.1 | 58.1 | — | — | — | 3.2× |
| `min_s` | 18.1 | 55.6 | — | — | — | 3.1× |
| `max_u` | 17.8 | 58.2 | — | — | — | 3.3× |
| `max_s` | 17.8 | 55.6 | — | — | — | 3.1× |
| `move` | 14.8 | 53.8 | 35.5 | 0.5 | 2.4× | 3.6× |
| `bswap` | 16.2 | 53.8 | 51.7 | 0.8 | 3.2× | 3.3× |
| `zext` | 15.0 | 53.7 | 31.2 | 0.5 | 2.1× | 3.6× |
| `sext_w` | 12.4 | 52.3 | — | — | — | 4.2× |
| `trunc` | 10.7 | 38.7 | 17.1 | 0.3 | 1.6× | 3.6× |
| `signext` | 23.1 | 53.8 | — | — | — | 2.3× |
| `load` | 20.5 | 1.2 | — | — | — | 0.1× |
| `store` | 15.4 | 1.4 | — | — | — | 0.1× |
| `div_u` | 650.7 | 1,026.5 | — | — | — | 1.6× |
| `div_s` | 668.5 | 1,060.7 | — | — | — | 1.6× |
| `rem_u` | 654.6 | 1,033.9 | — | — | — | 1.6× |
| `rem_s` | 669.1 | 1,067.9 | — | — | — | 1.6× |
| `addmod` | 1,292.1 | 1,936.2 | — | — | — | 1.5× |
| `exp` | 1,737.7 | 4,124.2 | — | — | — | 2.4× |
| `mulmod` | 1,893.8 | 3,162.0 | — | — | — | 1.7× |

On the compute ops the recompiler is ~2–4× the interpreter: it routes each through the `syscall_wide` trampoline as an **out-of-line call** (~40–60 ns: ~23 ns shared arithmetic + register save/restore), while the interpreter runs the same code inline (~15–36 ns). Against the recompiler's own **inline scalar reference** (a few ns), the extension is a modest loss for cheap ops — a wide `add` (58 ns) is ~8 inline scalar adds — which is what motivates inlining `adc`/`sbb` (§10). **Load/store invert** (generated inline on the recompiler, ~1–3 ns). One interpreted wide op replaces a whole scalar chain (the `ext vs ref` column). 128-bit speedups are smaller — a shorter chain to replace.

> **Measurement correction.** An earlier version of this table reported ~1,700 ns for every recompiler compute op (a flat "84–158× interp" column). That was an artifact of the microbench harness: `wide_microbench.rs` runs as a `#[test]`, and `is_sandbox_logging_enabled()` returns `cfg!(test) || …`, forcing the sandbox worker's trace logging on — so the zygote's `syscall_wide` did two `write()` syscalls plus a host wake **per wide op** (~1,680 ns of log I/O + a context switch), width-independent, swamping the real cost. The numbers above are re-measured with worker logging off (as `resolc`/`runblob` run); the real out-of-line cost is tens of nanoseconds. The interpreter and §6 (`runblob`, not a test build) were never affected.

## 5b. Recompiler native-code size per instruction (the per-instruction JIT budget)

The recompiler emits native x86-64 for each guest instruction into a per-instruction slot bounded by **`VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH` = 96 bytes** (polkavm-common `zygote.rs`; raised from 69 to admit the inline wide lowerings). That constant sizes the reserved native-code arena (`VM_SANDBOX_MAXIMUM_NATIVE_CODE_SIZE ≥ VM_MAXIMUM_CODE_SIZE × 96`) and is a **runner-side JIT limit — not the VM ABI, and not the on-chain blob**: a wide op is one instruction in the shipped blob regardless of how the recompiler lowers it. Bytes below are measured per-instruction emission for the **trampoline path** (the fallback, and what `POLKAVM_DISABLE_WIDE_INLINE` forces for every compute op). Compute ops route through the shared `syscall_wide` trampoline — a fixed call-site plus a one-time trampoline body that is emitted once and does *not* count per-instruction — so they are width-independent; load/store are generated inline and grow with width. The cheap/common compute ops now **default to inline native code** instead (larger per-instruction, at parity with the base ISA — see the inline table below and §6c).

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

Every instruction as currently implemented fits the budget with headroom (max = load/store at 52 B for i256). The trampoline keeps compute ops tiny at the call site (10–17 B) at the cost of the runtime crossing measured in §5.

**Inlining the wide compute ops (now the default; kill switch `POLKAVM_DISABLE_WIDE_INLINE`).** The recompiler generates the cheap/common wide ops as inline native code instead of the `syscall_wide` trampoline, using only `rcx` (`TMP_REG`) so nothing spills, and falling back to the trampoline for the heavy and i512+ ops. With the per-instruction cap at 96 B these inline at i256:

- **`add`/`sub`/`and`/`or`/`xor`** — *in-place* (`mov rcx,[other_i]; <op>/<carry-op> [d_i],rcx`, 2 instr/limb) when the destination aliases a source (`add`/`sub` chain the carry/borrow through `adc`/`sbb`; the `mov` preserves the flags); fits at any width. *Three-address* (`mov rcx,[s1_i]; <op> rcx,[s2_i]; mov [d_i],rcx`, 3 instr/limb) for distinct registers — i256 is 88 B, within the 96-byte cap.
- **`trunc`** — the result is `s1`'s low limb: one `mov gpr, [s1_0]`. Any width.
- **`zext`** — `mov [d_0], gpr; xor rcx,rcx; mov [d_i],rcx…`: scalar into limb 0, zero the rest. ≤ i256.
- **`move`** — limb copy `mov rcx,[s1_i]; mov [d_i],rcx` (a no-op when `d` aliases `s1`). ≤ i256.

Measured native bytes (exact) and correctness — every case cross-checked bit-for-bit against the scalar reference (`= yes`), including the memory-destination `adc`/`sbb`, the zero-fill, and the low-limb paths:

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

The fit is not cleverness left on the table; it follows from concrete PolkaVM recompiler constraints:

1. **Per-instruction native-code budget = 96 B** (raised from 69). `VM_COMPILER_MAXIMUM_INSTRUCTION_LENGTH` (polkavm-common `zygote.rs`) caps the native code one guest instruction may emit and sizes the reserved arena: `VM_SANDBOX_MAXIMUM_NATIVE_CODE_SIZE (3200 MiB) ≥ VM_MAXIMUM_CODE_SIZE (32 MiB) × 96`. The native-code region lives in `[VM_ADDR_NATIVE_CODE = 4 GiB, VM_ADDR_VMCTX = 16 GiB)` — a 12 GiB gap — so the 3200 MiB reservation (address space only, PROT_NONE, not committed) has ample room; the constant is runner-side and non-ABI. This admits the i256 distinct-register arith (88 B) and i256 `seq`/`sne` (~74 B); i512+ (≥ 112 B) still exceeds it and stays on the trampoline.
2. **Register scarcity.** The recompiler leaves exactly **one general scratch — `rcx`** — plus three wide temporaries (`T0`/`T1`/`T2`) that must be `push`/`pop`-preserved. A distinct 3-address i256 op juggles three operand streams; with one free scratch and 4-byte `disp32` offsets that is 12 memory instructions ≈ 84 B, and there aren't enough free registers for a compact `disp8` (base-pointer) encoding.
3. **No shift instruction, no flag-preserving loop.** The assembler implements neither `sar`/`shr`/`shl` (so the wide shifts and `sext`'s sign mask can't be built) nor `loop`/`jrcxz` (so a width-independent ~62 B carry loop — which would fit any width — is unavailable, and `rcx` is already the accumulator).
4. **Deep VmCtx layout → `disp32`.** The wide register file sits far into `VmCtx`, so every limb access carries a 4-byte displacement and there's no free register to hold a nearer base pointer.
5. **Overlap safety.** The trampoline's shared dispatch copies operands into temporaries, tolerating any overlap; the inline forms compute in place and assume operands are **identical or disjoint** (well-formed codegen never emits a partially-overlapping wide operand).

Net: with the cap at 96 B, inline wide compute is feasible and correct at i256 for `trunc`/`zext`/`move`, `add`/`sub`/`and`/`or`/`xor` (any register layout — in-place 2 instr/limb or three-address 88 B), and the compares `slt_u`/`slt_s`/`seq`/`sne` — the cheap/common majority of wide ops. The remaining ops stay on the trampoline — the heavy ones by design (limitation 1's list), the shifts/`min`/`max`/`bswap`/`sext` until the assembler grows shift and flag-preserving-loop primitives (limitations 2–3), and i512+ until a further cap raise. **The measured whole-contract effect is in §6c: inline brings the extension to parity with the base ISA on the recompiler (1.00× ref aggregate, −23% vs the trampoline).**

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

Per-call recompiler time (µs), 64 modules, **median of `min`-of-300 across 5 sweeps** per arm (fixed harness, recalibrated gas, full i256 inline). These times (~2–3 µs) are dominated by fixed per-call harness overhead — instance re-entry and host-call round-trips — which swamps the ~sub-µs of guest execution the extension changes. **So per-contract rows carry ~±15% noise; only the aggregate is reliable**, and the deterministic per-contract cost is gas (§6d), not this.

| | recomp total (ms) | vs ref |
|---|--:|--:|
| ref (no extension) | 0.19 | 1.00× |
| ext — trampoline | 0.24 | 1.30× |
| **ext — inline** | **0.19** | **1.00×** |

ext-inline is **1.00× ref aggregate, 1.00× median** — parity. Only 3/64 contracts reach 1.10–1.11×, all memory-op modules with no wide arithmetic (ratios within the ±15% floor); the extension carries no compute regression once the cheap/common wide ops inline.

**Inlining erased the ERC20 regression.** ERC20 moves 256-bit balances (load/store/`move`) and does 256-bit compares — statically **21 `slt_u` and 37 `seq`**. On the trampoline it was 1.15× ref. Inlining closed it in two steps: `slt_u`/`slt_s` first (→ 1.10×), then — after raising the per-instruction cap to 96 B — i256 `seq`/`sne` and distinct-register i256 arith (→ **1.03×**, this sweep; verified 15-run median ref 3736 / ext-inline 3906 ns, ext-inline's *minimum* 2633 ns below ref's). That is parity within the harness noise floor. The small residual is the wide file's memory residency on the 166 loads + 135 stores (addressed by the §7 narrowing transform); gas remains ~1.3× ref for the same reason (extra convert ops, not metering).

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

Gas is deterministic and backend-independent — it is what a chain charges. An earlier cost model added a fixed `WIDE_MARSHAL = 8` (modelling the trampoline crossing) to every wide op, which over-metered the cheap convert/memory/move ops 6–24× versus the scalar ops they replace, giving a **+2.47×** aggregate gas regression on the deploy/revert paths this corpus exercises. The costs are now recalibrated to the scalar limb chain (the naive model meters scalar at 1 gas/instruction) with no trampoline term:

| wide op class | old gas | new gas | scalar it replaces (256-bit) |
|---|--:|--:|--:|
| `trunc`/`zext`/`sext` (convert) | 12 | **2** | ~1–4 |
| `move` | 24 | **4** | ~4 |
| `load`/`store` (memory) | 24 | **6** | ~4–5 |
| `add`/`sub`/bitwise/min/max/cmp/shift/bswap (linear) | 24 | **16** | ~8–32 |
| `mul` | 64 | 56 | ~64 |
| `div`/`rem` | 3100/3200 | 3100/3200 | ~2,600 |
| `addmod`/`mulmod` | 6300 | 6300 | ~5,000 |
| `exp` | 8000 | 8000 | ~8,000 |

The "scalar it replaces" column is the approximate 64-bit-instruction count a base-ISA build's routine executes for the same 256-bit operation. The iterative ops are genuinely in the thousands: `dispatch` runs shift-and-subtract division as **n·64 = 256 iterations** each doing O(n) limb work (~2,600 primitive ops); the fused modular forms build a 512-bit intermediate and reduce it bit by bit (~5,000); `exp` is square-and-multiply, up to 256 iterations of two modular multiplies (~8,000). Those large costs are kept — they are the real work.

**Result:** aggregate ext/ref gas drops from **2.47× → 1.18×** (median 1.18×, min 0.96× — one contract now *below* ref, max 1.58×). The residual ~18% is not metering — it is the extra convert ops the codegen emits (values promoted to i256 then truncated back; the narrowing transform in §7 would remove them), so gas now tracks actual work. Heavy arithmetic keeps its large, correct costs.

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

Most contracts are unchanged (inference finds nothing to narrow); the effect concentrates in arithmetic ones (`Uint128Arithmetic`: −44% blob, ~6.5× slower interp). **Type inference trades a little code size for speed** — it narrows `i256`→`i64`/`i128`, producing larger but faster code, because scalar ops avoid the expensive wide path.

## 10. Status and next

- Working end to end on both backends; full polkavm suite passes.
- **Inline the cheap wide compute in the recompiler — DONE, now the default.** The cheap/common wide ops (`add`/`sub`/`and`/`or`/`xor`, `slt_u/s`, `seq`/`sne`, `trunc`/`zext`/`move`) generate inline native code using only `rcx`; heavy (`mul`/`div`/`rem`/`exp`/`mod`/`signext`), shift/`min`/`max`/`bswap`, and i512+ still route through the `syscall_wide` trampoline. This required raising the per-instruction cap 69→96 B (§5b limitation 1). Whole-contract effect: **1.00× ref aggregate on the recompiler, −23% vs the trampoline** (§6c); the ERC20 regression (1.15×) is erased. Kill switch: `POLKAVM_DISABLE_WIDE_INLINE`. Still open: shifts/`min`/`max`/`bswap`/`sext` need new assembler primitives; i512+ needs a further cap raise.
- **Disable the machine outliner** (§8). **Wide gas costs recalibrated** (§6d): the trampoline-marshal term over-metered cheap ops, giving a +2.47× gas regression; recalibrating to the scalar limb chain brings it to +1.18×, the residual being extra convert ops that the narrowing transform (§7) would remove.
- **Open bug:** a wide instruction directly after a call has no width the dataflow can supply (3 XENCrypto modules).
- **NewYork IR** needs a much faster Yul→IR stage before it can be benchmarked at scale.

*Harnesses/data in `benchmarks/analysis/vsetivli-tradeoff/`: `wide_microbench.rs`, `measure_per_bench.py`, `compare_ti.py`, `per-bench-default.tsv`, `ti-compare-results.txt`, `usage-default-counts.txt`, `microbench-{interp,recomp}.txt`.*
