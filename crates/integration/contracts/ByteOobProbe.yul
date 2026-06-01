/// Probe: `byte(32, x)` — byte index 32 is out of range. EVM returns 0.
/// revive's `byte` builds a branchless mask; the question is whether the
/// truncate-to-i8 of a constant 32 + shifts produces any UB intermediate.
object "ByteOobProbe" {
    code {
        let size := datasize("ByteOobProbe_deployed")
        codecopy(0, dataoffset("ByteOobProbe_deployed"), size)
        return(0, size)
    }
    object "ByteOobProbe_deployed" {
        code {
            let tag := calldataload(0)
            let value := calldataload(32)
            let idx := shl(5, 1)  // = 32
            mstore(0, xor(byte(idx, value), tag))
            return(0, 32)
        }
    }
}
