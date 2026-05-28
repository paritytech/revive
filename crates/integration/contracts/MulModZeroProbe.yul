/// Probe: `mulmod(a, b, 0)` — sibling of `AddModZeroProbe`. EVM = 0.
object "MulModZeroProbe" {
    code {
        let size := datasize("MulModZeroProbe_deployed")
        codecopy(0, dataoffset("MulModZeroProbe_deployed"), size)
        return(0, size)
    }
    object "MulModZeroProbe_deployed" {
        code {
            let tag := calldataload(0)
            let a := calldataload(32)
            let b := calldataload(64)
            let modulus := sub(shl(8, 1), shl(8, 1))
            mstore(0, xor(mulmod(a, b, modulus), tag))
            return(0, 32)
        }
    }
}
