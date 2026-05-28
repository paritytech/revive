/// Probe: `exp(0, 0)` — EVM defines `0^0 = 1`. revive's stdlib `__exp` checks
/// `exp == 0` first and returns 1; the body never runs for this case.
object "ExpZeroZeroProbe" {
    code {
        let size := datasize("ExpZeroZeroProbe_deployed")
        codecopy(0, dataoffset("ExpZeroZeroProbe_deployed"), size)
        return(0, size)
    }
    object "ExpZeroZeroProbe_deployed" {
        code {
            let tag := calldataload(0)
            let base := sub(shl(8, 1), shl(8, 1))      // = 0
            let exponent := sub(shl(8, 1), shl(8, 1))  // = 0
            mstore(0, xor(exp(base, exponent), tag))
            return(0, 32)
        }
    }
}
