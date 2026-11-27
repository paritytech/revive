object "StoreConstOffsetAbove32BitsAllNative" {
    code {
    }
    object "StoreConstOffsetAbove32BitsAllNative_deployed" {
        code {
            mstore(0x100000000, 7)
            sstore(0, mload(0))
        }
    }
}
