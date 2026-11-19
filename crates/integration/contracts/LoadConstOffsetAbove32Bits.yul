object "LoadConstOffsetAbove32Bits" {
    code {
        datacopy(0, dataoffset("LoadConstOffsetAbove32Bits_deployed"), datasize("LoadConstOffsetAbove32Bits_deployed"))
        return(0, datasize("LoadConstOffsetAbove32Bits_deployed"))
    }
    object "LoadConstOffsetAbove32Bits_deployed" {
        code {
            mstore(0, 7)
            sstore(0, mload(0x100000000))
        }
    }
}
