object "LoadConstOffsetAbove32Bits" {
    code {
    }
    object "LoadConstOffsetAbove32Bits_deployed" {
        code {
            mstore(0, 7)
            sstore(0, mload(0x100000000))
        }
    }
}
