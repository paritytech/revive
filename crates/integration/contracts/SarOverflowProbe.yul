/// Probe: `sar(256, INT_MIN)` — LLVM `ashr i256 x, 256` is UB; EVM defines SAR
/// with count >= 256 as 0 (positive) or -1 (negative, sign-fill). Tests the
/// negative branch by using INT_MIN as the dividend.
object "SarOverflowProbe" {
    code {
        let size := datasize("SarOverflowProbe_deployed")
        codecopy(0, dataoffset("SarOverflowProbe_deployed"), size)
        return(0, size)
    }
    object "SarOverflowProbe_deployed" {
        code {
            let tag := calldataload(0)
            let value := shl(255, 1)  // INT_MIN
            let count := shl(8, 1)
            mstore(0, xor(sar(count, value), tag))
            return(0, 32)
        }
    }
}
