object "StoreConstOffsetAbove32Bits" {
    code {
    }
    object "StoreConstOffsetAbove32Bits_deployed" {
        code {
            mstore(0x100000000, 7)
            sstore(0, mload(0))
        }
    }
}
