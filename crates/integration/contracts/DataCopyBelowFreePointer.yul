object "DataCopyBelowFreePointer" {
    code {
        mstore(0x40, 0xffffffffffffffffffffffffffffffff00000000000000000000000000000080)
        for { let i := 0 } lt(i, calldataload(0)) { i := add(i, 1) } { mstore(0x80, i) }
        datacopy(0x30, dataoffset("DataCopyBelowFreePointer_deployed"), 0x10)
        for { let j := 0 } lt(j, calldataload(0)) { j := add(j, 1) } { mstore(0x80, j) }
        let a := mload(0x40)
        mcopy(0xa0, 0x40, 0x20)
        if iszero(eq(a, mload(0xa0))) { revert(0, 0) }
        return(0, 0)
    }
    object "DataCopyBelowFreePointer_deployed" {
        code { stop() }
    }
}
