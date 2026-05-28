/// Probe: `shr(256, x)` — LLVM `lshr i256 x, 256` is UB; EVM defines SHR with
/// count >= 256 as 0.
object "ShrOverflowProbe" {
    code {
        let size := datasize("ShrOverflowProbe_deployed")
        codecopy(0, dataoffset("ShrOverflowProbe_deployed"), size)
        return(0, size)
    }
    object "ShrOverflowProbe_deployed" {
        code {
            let tag := calldataload(0)
            let value := calldataload(32)
            let count := shl(8, 1)
            mstore(0, xor(shr(count, value), tag))
            return(0, 32)
        }
    }
}
