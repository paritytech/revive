object "DataCopyZeroLength" {
    code {
        mstore(0x80, 0x0102030405060708091011121314151600000000000000000000000000000000)
        datacopy(0x90, dataoffset("DataCopyZeroLength_deployed"), 0)
        for { let i := 0 } lt(i, calldataload(0)) { i := add(i, 1) } { }
        if iszero(eq(mload(0x80), 0x0102030405060708091011121314151600000000000000000000000000000000)) { revert(0, 0) }
        return(0, 0)
    }
    object "DataCopyZeroLength_deployed" {
        code { stop() }
    }
}
