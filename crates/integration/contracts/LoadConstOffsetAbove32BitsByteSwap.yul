object "LoadConstOffsetAbove32BitsByteSwap" {
    code {
        datacopy(0, dataoffset("LoadConstOffsetAbove32BitsByteSwap_deployed"), datasize("LoadConstOffsetAbove32BitsByteSwap_deployed"))
        return(0, datasize("LoadConstOffsetAbove32BitsByteSwap_deployed"))
    }
    object "LoadConstOffsetAbove32BitsByteSwap_deployed" {
        code {
            mstore(0, 7)
            sstore(0, mload(0x100000000))
        }
    }
}
