/// Soundness PoC candidate: store a meaningful value into the free-memory-
/// pointer slot (0x40) and return a region covering it. If heap native-mode
/// stores 0x40 little-endian while the `return` reads it big-endian, the
/// returned bytes are byte-swapped relative to EVM.
object "FmpSlotReturnByteOrder" {
    code {
        let size := datasize("FmpSlotReturnByteOrder_deployed")
        codecopy(0, dataoffset("FmpSlotReturnByteOrder_deployed"), size)
        return(0, size)
    }
    object "FmpSlotReturnByteOrder_deployed" {
        code {
            let v := calldataload(0)
            mstore(0x40, v)
            return(0x40, 32)
        }
    }
}
