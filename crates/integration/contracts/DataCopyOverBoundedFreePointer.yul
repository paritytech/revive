/// The hash of a child object written over a bounded free memory pointer must reach the pointer read in full.
object "DataCopyOverBoundedFreePointer" {
    code {
        mstore(0x40, 0x80)
        for { let i := 0 } lt(i, calldataload(0)) { i := add(i, 1) } { mstore(0x80, i) }
        datacopy(0x30, dataoffset("DataCopyOverBoundedFreePointerChild"), 0x10)
        for { let j := 0 } lt(j, calldataload(0)) { j := add(j, 1) } { mstore(0x80, j) }
        let a := mload(0x40)
        mcopy(0xa0, 0x40, 0x20)
        if iszero(eq(a, mload(0xa0))) { revert(0, 0) }
        return(0, 0)
    }
    object "DataCopyOverBoundedFreePointer_deployed" {
        code { stop() }
    }
    object "DataCopyOverBoundedFreePointerChild" {
        code { return(0, 0) }
        object "DataCopyOverBoundedFreePointerChild_deployed" {
            code { stop() }
        }
    }
}
