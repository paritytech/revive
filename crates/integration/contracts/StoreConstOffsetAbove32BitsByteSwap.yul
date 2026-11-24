object "StoreConstOffsetAbove32BitsByteSwap" {
    code {
        datacopy(0, dataoffset("StoreConstOffsetAbove32BitsByteSwap_deployed"), datasize("StoreConstOffsetAbove32BitsByteSwap_deployed"))
        return(0, datasize("StoreConstOffsetAbove32BitsByteSwap_deployed"))
    }
    object "StoreConstOffsetAbove32BitsByteSwap_deployed" {
        code {
            mstore(0x100000000, 7)
            sstore(0, mload(0))
        }
    }
}
