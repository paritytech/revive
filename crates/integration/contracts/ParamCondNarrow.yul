/// Soundness PoC: newyork parameter narrowing
/// (`type_inference.rs::narrow_function_params`) narrows a parameter to I64/I32
/// whenever its `max_width` is lowered below I256 by a use that re-validates the
/// value (a memory offset bounded by `safe_truncate_int_to_xlen`). The call
/// boundary then emits a *checked* truncation that traps on an out-of-range
/// argument (`to_llvm.rs::checked_truncate_to`).
///
/// The bug: `max_width` was lowered by ANY narrowing use, including one reached
/// only on a conditional path. `store_if(p, c)` uses `p` as a memory offset only
/// inside `if c`. EVM skips the store (and never touches `p`) when `c == 0`, so a
/// `p >= 2^64` argument is harmless. But the narrowed `i64` signature truncates
/// `p` unconditionally at every call site, trapping before `c` is even tested.
///
/// Every caller passes a `calldataload`-derived `p` (forward width I256, so the
/// caller-driven narrowing path cannot narrow it) and four distinct call sites
/// plus the body's arithmetic chain keep `store_if` out of the inliner, so the
/// narrowed parameter reaches codegen.
///
/// Called with `c == 0` and `p == 2^64`: EVM returns `4` (each `store_if`
/// returns its else-path value); the buggy narrowing traps at the first call.
object "ParamCondNarrow" {
    code {
        let size := datasize("ParamCondNarrow_deployed")
        codecopy(0, dataoffset("ParamCondNarrow_deployed"), size)
        return(0, size)
    }
    object "ParamCondNarrow_deployed" {
        code {
            let c := calldataload(0)
            let acc := 0
            acc := add(acc, store_if(calldataload(32), c))
            acc := add(acc, store_if(calldataload(64), c))
            acc := add(acc, store_if(calldataload(96), c))
            acc := add(acc, store_if(calldataload(128), c))
            mstore(0, acc)
            return(0, 32)

            function store_if(p, g) -> r {
                r := 1
                if g {
                    // `p` is used as a memory offset only on this conditional path.
                    mstore(p, 0x42)
                    r := mload(p)
                }
                // Live arithmetic chain on the condition keeps the body over the
                // always-inline size so the narrowed parameter reaches codegen.
                let a1 := add(g, 1)
                let a2 := mul(a1, 3)
                let a3 := xor(a2, 7)
                let a4 := add(a3, 9)
                let a5 := or(a4, 13)
                let a6 := add(a5, 17)
                let a7 := mul(a6, 19)
                let a8 := xor(a7, 23)
                r := add(r, a8)
            }
        }
    }
}
