/// Probe: `shl(256, x)` — LLVM `shl i256 x, 256` is UB; EVM defines SHL with
/// count >= 256 as 0. revive's `shift_left` adds a switch+phi guard; this
/// fixture stresses whether it survives O3 const-folding.
object "ShlOverflowProbe" {
    code {
        let size := datasize("ShlOverflowProbe_deployed")
        codecopy(0, dataoffset("ShlOverflowProbe_deployed"), size)
        return(0, size)
    }
    object "ShlOverflowProbe_deployed" {
        code {
            let tag := calldataload(0)
            let value := calldataload(32)
            // count = 256, obfuscated so revive can't fold pre-LLVM
            let count := shl(8, 1)
            mstore(0, xor(shl(count, value), tag))
            return(0, 32)
        }
    }
}
