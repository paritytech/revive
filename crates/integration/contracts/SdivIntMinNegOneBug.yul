/// Dedicated fixture exercising `sdiv(INT_MIN, -1)` — the signed-overflow UB
/// case for division, sibling to the `smod` bug in paritytech/revive#524.
///
/// LLVM `sdiv` is UB when the dividend is `INT_MIN` and the divisor is `-1`
/// (the mathematical result `-INT_MIN` overflows). EVM SDIV defines this as
/// `INT_MIN`. revive's `__revive_signed_division` runtime function is supposed
/// to guard this; the issue says sdiv is already guarded but having a test
/// here protects against regressions and surfaces any future divergence.
///
/// See `SmodIntMinNegOneBug.yul` for the rationale behind the Yul expression
/// operands and the lack of a `switch` dispatcher.
object "SdivIntMinNegOneBug" {
    code {
        let size := datasize("SdivIntMinNegOneBug_deployed")
        codecopy(0, dataoffset("SdivIntMinNegOneBug_deployed"), size)
        return(0, size)
    }
    object "SdivIntMinNegOneBug_deployed" {
        code {
            let tag := calldataload(0)
            let int_min := shl(255, 1)
            let neg_one := sar(58, shl(58, sub(0, 1)))
            mstore(0, xor(sdiv(int_min, neg_one), tag))
            return(0, 32)
        }
    }
}
