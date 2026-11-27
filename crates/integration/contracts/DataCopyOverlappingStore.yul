object "DataCopyOverlappingStore" {
    code {
        mstore(0x80, 0xffffffffffffffffffffffffffffffff00000000000000000000000000000080)
        datacopy(0x70, dataoffset("DataCopyOverlappingStore_deployed"), 0x10)
        let a := mload(0x80)
        mcopy(0xc0, 0x80, 0x20)
        if iszero(eq(a, mload(0xc0))) { revert(0, 0) }
        return(0, 0)
    }
    object "DataCopyOverlappingStore_deployed" {
        code { stop() }
    }
}
