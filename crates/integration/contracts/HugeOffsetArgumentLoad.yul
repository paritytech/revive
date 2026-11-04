/// A memory offset passed to a function whose offset parameter is narrowed to 64 bits must still
/// trap at the call site when it does not fit. A huge offset makes EVM run out of gas on memory
/// expansion. The callee only loads, so the free memory pointer stays bounded and this exercises
/// the regular narrowing pipeline rather than the unbounded free memory pointer re-inference.
/// The deploy code copies to `0x80` so it does not cover the free memory pointer word either.
/// The loop with a `leave` keeps `load` from being inlined.
object "HugeOffsetArgumentLoad" {
    code {
        let size := datasize("HugeOffsetArgumentLoad_deployed")
        codecopy(0x80, dataoffset("HugeOffsetArgumentLoad_deployed"), size)
        return(0x80, size)
    }
    object "HugeOffsetArgumentLoad_deployed" {
        code {
            mstore(0x40, 0x80)
            mstore(0, load(calldataload(0)))
            return(0, 32)

            function load(offset) -> value {
                for { let i := 0 } lt(i, 2) { i := add(i, 1) } {
                    if eq(i, 7) { leave }
                }
                value := mload(offset)
            }
        }
    }
}
