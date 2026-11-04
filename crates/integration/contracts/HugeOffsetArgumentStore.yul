/// A memory offset passed to a function whose offset parameter is narrowed to 64 bits must still
/// trap at the call site when it does not fit. A huge offset makes EVM run out of gas on memory
/// expansion. The dynamic store makes the free memory pointer possibly unbounded, so this
/// exercises the signature re-narrowing after the unbounded free memory pointer re-inference.
/// The loop with a `leave` keeps `store` from being inlined.
object "HugeOffsetArgumentStore" {
    code {
        let size := datasize("HugeOffsetArgumentStore_deployed")
        codecopy(0, dataoffset("HugeOffsetArgumentStore_deployed"), size)
        return(0, size)
    }
    object "HugeOffsetArgumentStore_deployed" {
        code {
            mstore(0x40, 0x80)
            store(calldataload(0), 0xdead)
            mstore(0, mload(0x40))
            return(0, 32)

            function store(offset, value) {
                for { let i := 0 } lt(i, 2) { i := add(i, 1) } {
                    if eq(i, 7) { leave }
                }
                mstore(offset, value)
            }
        }
    }
}
