object "LoadConstOffsetAbove32BitsAllNative" {
    code {
    }
    object "LoadConstOffsetAbove32BitsAllNative_deployed" {
        code {
            mstore(0, 7)
            sstore(0, mload(0x100000000))
        }
    }
}
