object "StoreConstOffsetAbove32Bits" {
    code {
        datacopy(0, dataoffset("StoreConstOffsetAbove32Bits_deployed"), datasize("StoreConstOffsetAbove32Bits_deployed"))
        return(0, datasize("StoreConstOffsetAbove32Bits_deployed"))
    }
    object "StoreConstOffsetAbove32Bits_deployed" {
        code {
            mstore(0x100000000, 7)
            sstore(0, mload(0))
        }
    }
}
