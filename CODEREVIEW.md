# cl/newyork code review

Branch adds `crates/newyork/` (~25K LOC) plus `crates/fuzzer/` (~1K) and ~4.5K
of planning markdowns. Goal of this review: identify code that can be
**deleted**, not relocated. File splits don't count.

All line counts in this document are from the working tree on cl/newyork
(commit 6226524f). Numbers prefixed with `~` are estimates from grep-pattern
counting; numbers without are exact `wc -l` output.

## 1. Where the lines actually are

```
6671  to_llvm.rs        (codegen — 3 giant match functions)
4410  simplify.rs       (simplifier + 2 dedup passes + 2 outliners + DCE + keccak fold)
2224  inline.rs         (IR-level inliner with 5 walker families)
2123  type_inference.rs (forward/backward width inference, 6 walkers)
1744  mem_opt.rs        (load-after-store, FMP propagation, 4 walkers)
1565  from_yul.rs       (Yul AST → newyork IR)
1341  printer.rs        (Display impls)
1301  ir.rs             (core types — no visitor abstraction)
1082  heap_opt.rs       (offset escape analysis, 3 walkers)
 971  guard_narrow.rs   (validator-mask narrowing, ~650 LOC walker boilerplate)
 883  validate.rs       (SSA + region validity)
 493  compound_outlining.rs (mapping_sload/sstore detection, 290 LOC walker)
 262  lib.rs
 185  ssa.rs
```

Plus untracked planning markdowns at workspace root: `IR_PLAN.md` 1781,
`RALPH_TASK.md` 1835, `OPT_FINDINGS_AGENT_ONE.md` 138,
`OPT_FINDINGS_AGENT_TWO.md` 332, `crates/newyork/newyork_status.md` 269.
Total: ~4355 lines of AI scratchpad in the diff.

## 2. The dominant anti-pattern: parallel IR walkers

Every analysis has its own `*_block` / `*_region` / `*_statement` / `*_expr`
walker. Each walker is a 25-arm match over `Statement` plus a smaller match
over `Expr`, recursing into nested regions. There is no shared traversal
infrastructure in `ir.rs`.

Confirmed walker families (one bullet = one match-every-variant family):

- `simplify.rs`: `collect_used_in_*` (1881–2174), `Canonicalizer::encode_*`
  (2519–3030), `fuzzy_encode_*` (3390–3606), `collect_literals_*` (3608–3691),
  `replace_literals_*` (3693–3805), `update_call_sites_*`/`rewrite_*`
  (3807–3941), `redirect_calls_*` (3944–4015), `fold_keccak_in_*` (4030–4122).
  **8 families.**
- `inline.rs`: `count_calls_in_*` (126–216), `count_leaves_*` (273–316),
  `stmt_has_leave_recursive` (822–900), `inline_in_*` (1597–1770),
  `estimate_*_size` (1914–1955). **5 families.**
- `guard_narrow.rs`: `replace_value_ids_in_expr/stmt`/`replace_region`
  (612–957) is **~350 LOC of pure rebuild-each-variant boilerplate**;
  `narrow_block/region/stmt_regions` (200–528) another **~300 LOC**.
- `mem_opt.rs`: `optimize_block/region/statements` plus `propagate_block/region/statements`
  plus `region_writes_fmp/statements_write_fmp`. **3 families.**
- `type_inference.rs`: `propagate_demands_*`, `collect_arg_widths_*`,
  `collect_return_demands_*`, `infer_function_forward`, `collect_uses_function`,
  `refine_demands_in_block`. **6 families.**
- `heap_opt.rs`: `analyze_block/region/statement` + `analyze_expr_*`. **2 families.**
- `compound_outlining.rs`: `count_uses_in_expr/region/stmt` (104–389) is
  **~290 LOC** in this 493-LOC file.
- `validate.rs`: dominance-walk + region-walk. ~100 LOC.

Each new pass adds another walker. The cost compounds.

## 3. Mechanical deletion (no behavior change, gated by retester+codesize)

These are real LOC reductions, not file moves. Numbers are conservative
estimates based on counting near-identical match arms.

### 3.1 Visitor traits in `ir.rs`

Add `Visit` and `VisitMut` traits with default impls that recurse through
`Statement`/`Region`/`Expr`. Each walker family becomes a struct
implementing 1–3 hooks instead of 4 mutually-recursive functions.

Expected delta in each file (delete walker bodies, keep hook logic):
- simplify.rs: −700
- inline.rs: −250
- guard_narrow.rs: −500
- type_inference.rs: −200
- mem_opt.rs: −150
- heap_opt.rs: −150
- compound_outlining.rs: −200
- validate.rs: −50

Cost: ~+150–200 LOC of trait + default impls in `ir.rs`.

**Net: ~−2050 LOC.**

### 3.2 `Statement::map_value_ids` / `Expr::map_value_ids` on the IR types

In-place value-id substitution. Replaces:
- `replace_value_ids_in_stmt`/`_in_expr` in guard_narrow.rs (≈350 LOC)
- 18 passthrough arms in `simplify_statement` (256–717)
- Inliner's value remapping (`InlineRemapper` body, parts of inline.rs)

**Net: ~−400 LOC.**

### 3.3 Unify exact + fuzzy `Canonicalizer` in simplify.rs

`Canonicalizer::encode_*` (2519–3030) and `fuzzy_encode_*` (3390–3606) are
~90% identical. Difference: fuzzy replaces `Expr::Literal`, switch case
values, and `static_slot` constants with a counter. Add `LiteralMode` field
to `Canonicalizer`; one set of match arms.

**Net: ~−200 LOC.**

### 3.4 Tighten `simplify_binary` with `is_const_eq(Option<&BigUint>, u64)` helper

Pattern `rhs_val.as_ref().is_some_and(|v| v.is_zero())` repeats ~25 times in
1582–1771. Helper makes each rule one line.

**Net: ~−50 LOC.**

### 3.5 Cleanup of dead/wrong patterns in simplify.rs

- `match expr { ExtCodeSize | ExtCodeHash | Balance => ... _ => unreachable!() }`
  at 2628–2638: split into 3 arms.
- `nullary_expr_tag`'s `_ => 0x00` fallback at 3139: silent corruption,
  should be `unreachable!()` (or an `Option<u8>` return).
- The disabled-but-still-referenced `outline_error_string_patterns` mention
  at 192–194 and 736–738.

**Net: ~−30 LOC.**

### 3.6 Trivial tests testing the `num` crate

simplify.rs tests at 4124–4410 include `test_constant_fold_add` (100+200=300),
`test_constant_fold_mul` (7*6=42), `test_unary_fold` (NOT 0 = MAX) etc. These
test BigUint arithmetic, not our compiler logic.

Realistic delete: ~150 LOC. Keep tests that exercise the simplifier on actual
IR fragments (e.g. `test_simplifier_constant_propagation`).

**Net: ~−150 LOC.**

### Mechanical subtotal: ~−2880 LOC

## 4. Pass-deletion candidates (need experimental verification)

Each requires the same experiment: disable the pass, run
`RESOLC_USE_NEWYORK=1 cargo test --package revive-integration -- codesize`
plus `cd oz-tests && RESOLC_USE_NEWYORK=1 bash oz.sh`, record the byte delta.
Threshold: if disabling costs <500 bytes/site or <1% of OZ total
(320,669 baseline), delete.

| Pass | LOC | Theory for deletion |
|---|---|---|
| `simplify_binary` algebraic identities | ~200 | LLVM InstCombine handles `x+0`, `x*1`, `x*0`, `x|x`, `x^x`, etc. We may be paying twice. |
| `try_strength_reduce` (mul/div/mod by 2^k → shifts) | ~100 | LLVM does this in InstCombine + CodeGenPrepare. |
| `deduplicate_functions_fuzzy` | 470 | LLVM has MergeFunctions; no measured savings recorded in MEMORY.md for fuzzy specifically. |
| `compound_outlining.rs` whole crate | 493 | MEMORY.md records "−1833 bytes for ~500 LOC, on the edge". |
| `fold_constant_keccak` standalone pass (4030–4122) | ~100 | `simplify_expr` already folds Keccak256Single/Pair when args are constant; this is a redundant second walk. |
| `simplify` constant keccak folding inside `simplify_expr` | — | If `fold_constant_keccak` is the canonical one, the inline version is the redundancy instead. Pick one. |
| IR-level inliner (`inline.rs` whole) | 2224 | LLVM has an inliner. Question: does `make_inline_decisions` find anything LLVM doesn't, or just pre-empt LLVM's choices? Memory says heavy tuning (ALWAYS=6, SINGLE_CALL=20). Worth measuring. |
| `deduplicate_functions` (exact) | ~600 | LLVM MergeFunctions exists. Measure. |
| `guard_narrow.rs` whole crate | 971 | Pass narrows values guarded by `and(x, MASK)` patterns. Question: does LLVM's range/known-bits analysis catch the same cases? |

If half of these greenlight: ~−2000 LOC additional.

## 5. to_llvm.rs (6671 LOC) — separate audit needed

Three giant match functions:
- `generate_statement_inner` (3527–5390): ~1860 LOC, one arm per `Statement`
- `generate_expr` (5391–6274): ~880 LOC, one arm per `Expr`
- `generate_binop` (6325–6660): ~340 LOC, one arm per `BinOp`

Not walker boilerplate — actual codegen. But contains:
- Repeated `build_int_*` + safe-truncate plumbing patterns
- `BinOp` / `CmpOp` / shift dispatch that could be const tables (op → LLVM
  intrinsic + predicate + signedness)

Estimated potential: −1500 to −2000 LOC. Needs a focused audit, not yet done.

## 6. What this review is NOT counting as savings

- **File splits.** Moving `deduplicate_functions` from simplify.rs to a new
  `dedup.rs` deletes zero lines. Don't claim it.
- **Renaming or restructuring.** Same.
- **Comment removal.** The crate has surprisingly few rotted comments;
  removing them isn't material.
- **Test deletions beyond the trivial-num-crate ones.** Real test coverage
  stays.

## 7. Honest crate-level estimate

| Source | LOC delta |
|---|---|
| Mechanical (§3) | −2880 |
| Pass deletions, half greenlight (§4) | −2000 |
| to_llvm.rs audit, mid estimate (§5) | −1500 |
| **Net Rust LOC reduction** | **−6380** |
| Planning markdowns deletion (not Rust) | −4355 |

Crate goes from ~25K → ~19K Rust LOC (−24%). Plus 4.4K of markdown deleted.

**This is not halving.** Halving 25K of working compiler code without losing
features requires either (a) the to_llvm.rs audit producing more than the
mid estimate, or (b) accepting that some passes get deleted entirely
because LLVM does the same job acceptably.

The honest framing: "halve it" probably means "delete 2–3 whole passes and
do the visitor refactor." Decide which passes to sacrifice based on
measurement, not theory.

## 8. Plan order

1. Add `Visit` / `VisitMut` traits + default impls to `ir.rs`. Convert one
   small walker (e.g. `count_calls_in_*` in inline.rs) as proof. Land
   independently.
2. Add `Statement::map_value_ids` / `Expr::map_value_ids`. Convert
   `replace_value_ids_in_*` in guard_narrow.rs (biggest single win).
3. Fan out the visitor across simplify/inline/mem_opt/type_inference/
   heap_opt/compound_outlining/validate.
4. Unify exact + fuzzy `Canonicalizer`.
5. Tighten `simplify_binary`, fix oddities, drop trivial tests.
6. **Run pass-deletion experiments** (§4). For each candidate: disable,
   measure, decide.
7. Audit to_llvm.rs separately.
8. Delete the planning markdowns once the cl/newyork PR merges or after a
   final "is anyone using these" check.

Each step is independently committable and gated by
`RESOLC_USE_NEWYORK=1 make test` + the codesize test.
