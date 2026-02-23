# Compiler optimization task

The task: Identify a new (codesize) optimization opportunity and implement it.

WARNING. DO NOT UNDERESTIMATE THIS WORKLOAD.
- This is a complex task. More complex than usual. Senior compiler engineer level complexity.
- DO NOT oneshot this. Think thoroughly. Gather evidence (by looking at compiler artifacts) that your optimization idea will provide real gains (not some mere bytes).
- After commit, track progress (what was identified and implement or what does not work) in this document at the end!

Below is _very_ useful information to guide you with this task.

# Background

The newyork (new yul optimizer kit) is a new optimzing compiler pipeline. It bridges the gap between the solc optimizers and LLVM, neither of which are aware of PVM specifics. It is the natural place to implement optimization gaps.

# Invoking the `newyork` optimizer  pipeline 

Set the global environment variable RESOLC_USE_NEWYORK=1 enables it. This is universal, it works in normal resolc CLI mode, STD json mode and tests.

# Optimization verification

The main targets to optimize are openzepplin contracts. You can run `cd oz-tests && RESOLC_USE_NEWYORK=1 bash oz.sh` to compile them it will print the pvm bytecode sizes. The target of the newyork optimizer is significant size reduction like 50% smaller contract blobs against without it.

A very simple and limited code size test is found at `crates/integration/codesize.json`. This serves as a first indicator. This should not regress.


# Debugging and troubleshooting

resolc has `--debug-output-dir` to emit debug artifacts. You should inspect the Yul code as well as emitted optimized, unoptimized ll and also the PVM bytecode. So you can inspect how well code is translated or optimized between steps and get ideas for optimizations by inspecting intermediate representation artifacts and final bytecode.

You should be able to use stderr for logs and debugging optimization passes. Just make sure not to spam it as it will eat up ram and spam a lot. Don't bother with stdout at all. resolc spawns itself recursively and uses stdout for intra process communication so printing stuff to stdout will break things.

# Testing

IMPORTANT: Tests may require a current `resolc` built from current changes in $PATH! ALWAYS install it before running test suites!

`make test-integration` provides a fast way to verify sanity.
`bash retester.sh` is a comprehensive regression test suite with over 5k tests. 

Hint: Optimizations are tricky to implement. Run `make test-integration` often!

IMPROTANT. You are NOT allowed to commit without these two steps:
1. ALWAYS verify `RESOLC_USE_NEWYORK=1 make test` passes before commit
2. ALWAYS verify `RESOLC_USE_NEWYORK=1 bash retester.sh` has 0 failures before commit
3. Check the openzeppelin contracts in oz-tests as well as codesize.json - regressions together with overall gains are ok but in general code sizes must not regress!

# Progress tracking

## Measurements baseline
- Without newyork: 605,031 bytes (OZ contracts total)
- Target (50%): 302,515 bytes

## Iteration 1-2 (prior sessions)
- Implemented forward type inference (min_width), backward demand analysis (use_demand_width)
- Added comparison narrowing (Lt/Gt/Eq at i64 when provably narrow)
- Added SHR forward inference for constant shifts
- Added try_narrow_let_binding (truncate let-bound values based on proven/demand width)
- Added revert block sharing (deduplicate identical revert patterns)
- Added caller/callvalue/calldataload outlining
- Added function deduplication (exact + fuzzy)
- Result: 443,094 bytes (26.8% reduction)

## Iteration 3 (this session, prior context)
- Implemented demand-driven narrow codegen: pass demand_bits to generate_expr
  - Add/Sub/Mul at i64 when demand ≤ 64 (modular arithmetic preserves low bits)
  - And/Or/Xor at i64 when demand ≤ 64
  - Saved -6,166 bytes
- Implemented iterative parameter narrowing (up to 4 iterations)
  - narrow_function_params → refine_demands_from_params → cascade
  - Saved -1,757 bytes
- Modified apply_backward_constraints to use fn_arg_demand for FunctionArg contexts
- Failed experiments:
  - Per-region native memory: +10,088 bytes (REVERTED - mixed paths overhead)
  - LLVM machine outliner: +1,926 bytes (REVERTED - overhead exceeds savings)
- Result: 435,171 bytes (28.1% reduction)

## Iteration 4 (current context)
- Added demand-driven Shl narrowing (constant shift < 64 at i64 when demand ≤ 64)
- Added demand-driven Shr narrowing (when value provably ≤ 64 bits, constant shift < 64)
- Added demand-driven Not narrowing (at i64 when demand ≤ 64)
- Improved narrow_offset_for_pointer to narrow to i32 (xlen) when forward inference proves it, eliminating overflow check entirely
- Failed experiments:
  - Direct overflow check for intermediate widths in safe_truncate_int_to_xlen: REVERTED (each call site gets own trap block, LLVM can't merge them well, +121 bytes on Governor)
  - LLVM O3 optimization: +18% larger than Oz
  - LLVM Os optimization: +10% larger than Oz
- Current result: ~435,164 bytes (28.1% reduction, marginal improvement from Shl/Shr/Not)
- Key insight: 90.8% of code is contract logic, only 9.2% runtime helpers
- Key insight: 46% of PVM instructions are load/store (register spills from i256)
- Key bottleneck: heap/storage operations inherently use i256 (309 store + 186 load heap word calls per contract)

