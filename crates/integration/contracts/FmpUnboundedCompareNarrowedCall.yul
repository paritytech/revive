/// The free memory pointer is set from calldata, so it may be unbounded and its loads must stay
/// full width. That holds across the function signature narrowing that follows the unbounded
/// free memory pointer re-inference. `low_byte` has a narrowable return value, so the narrowing
/// iterates and re-runs type inference, and that re-run must not assume a bounded free memory
/// pointer. Otherwise `gt(fmp, 0xffffffff)` compares a truncated pointer. The loop with a `leave`
/// keeps `low_byte` from being inlined.
object "FmpUnboundedCompareNarrowedCall" {
    code {
        let size := datasize("FmpUnboundedCompareNarrowedCall_deployed")
        codecopy(0, dataoffset("FmpUnboundedCompareNarrowedCall_deployed"), size)
        return(0, size)
    }
    object "FmpUnboundedCompareNarrowedCall_deployed" {
        code {
            mstore(0x40, calldataload(0))
            let ok := staticcall(gas(), 0, 0, 0, 0, 0)
            let fmp := mload(0x40)
            let out := 0
            if gt(fmp, 0xffffffff) {
                out := 1
            }
            mstore(0, out)
            mstore(32, low_byte(calldataload(32)))
            return(0, 64)

            function low_byte(value) -> r {
                r := and(value, 0xff)
                for { let i := 0 } lt(i, 3) { i := add(i, 1) } {
                    if eq(i, value) { leave }
                }
            }
        }
    }
}
