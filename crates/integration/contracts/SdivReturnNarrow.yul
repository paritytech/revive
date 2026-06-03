/// Soundness PoC: newyork return-type narrowing
/// (`type_inference.rs::narrow_function_returns`) narrows a function's return
/// type to its forward `min_width` whenever `min_width < I256` and the return
/// value is not flagged `is_signed`.
///
/// The forward width rule for `sdiv` is `lhs_width` (the dividend's width)
/// (`infer_expression_width`: `Div | SDiv | Mod | SMod => lhs_width`). For
/// *unsigned* div/mod and for `smod` (result takes the dividend's sign) this is
/// a correct upper bound. For **signed division `sdiv`** it is NOT: when the
/// dividend is a small non-negative value (narrow width) but the divisor is
/// negative, the quotient is negative — a full-width two's-complement value
/// (~2^256). `sdiv` never marks its result `is_signed`, so the guard misses it.
///
/// `doSdiv` returns `sdiv(x, y)`; every caller passes `and(calldataload,0xff)`
/// so the dividend param `x` narrows, the return min_width is computed as
/// `x`'s width, and the return type is clamped/narrowed — truncating the
/// negative quotient. Four distinct call sites + body size keep the function
/// out of the inliner (CostBenefit -> LLVM noinline), so the narrowed return
/// survives to codegen.
///
/// `sdiv(5, -1) = -5 = 2^256 - 5` on EVM; the narrowed return drops the high
/// bits, returning 0x...fffffffb instead.
object "SdivReturnNarrow" {
    code {
        let size := datasize("SdivReturnNarrow_deployed")
        codecopy(0, dataoffset("SdivReturnNarrow_deployed"), size)
        return(0, size)
    }
    object "SdivReturnNarrow_deployed" {
        code {
            let z0, w0 := doSdiv(and(calldataload(0), 0xff), calldataload(32))
            let z1, w1 := doSdiv(and(calldataload(64), 0xff), calldataload(96))
            let z2, w2 := doSdiv(and(calldataload(128), 0xff), calldataload(160))
            let z3, w3 := doSdiv(and(calldataload(192), 0xff), calldataload(224))
            mstore(0, z0)
            mstore(32, w0)
            mstore(64, z1)
            mstore(96, w1)
            mstore(128, z2)
            mstore(160, w2)
            mstore(192, z3)
            mstore(224, w3)
            return(0, 256)

            function doSdiv(x, y) -> z, w {
                z := sdiv(x, y)
                // Live chain on the divisor `y` (kept observable via `w`) to
                // push the function body over the always-inline size, so the
                // narrowed return type reaches codegen.
                let a1 := add(y, 1)
                let a2 := mul(a1, 3)
                let a3 := xor(a2, 7)
                let a4 := add(a3, 9)
                let a5 := or(a4, 13)
                let a6 := add(a5, 17)
                let a7 := mul(a6, 19)
                let a8 := xor(a7, 23)
                w := add(a8, 29)
            }
        }
    }
}
