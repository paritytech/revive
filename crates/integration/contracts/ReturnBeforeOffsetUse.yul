object "ReturnBeforeOffsetUse" {
    code {
        datacopy(0, dataoffset("ReturnBeforeOffsetUse_deployed"), datasize("ReturnBeforeOffsetUse_deployed"))
        return(0, datasize("ReturnBeforeOffsetUse_deployed"))
    }
    object "ReturnBeforeOffsetUse_deployed" {
        code {
            mstore(0x40, 0x80)
            mstore(0, return_if_nonzero(calldataload(0)))
            mstore(0, return_if_nonzero(calldataload(32)))
            mstore(0, return_if_nonzero(calldataload(64)))
            return(0, 32)

            function load_nonzero(offset) -> value {
                value := mload(offset)
                if value { leave }
            }
            function return_if_nonzero(offset) -> value {
                if offset {
                    mstore(0, 1)
                    return(0, 32)
                }
                for { let i := 0 } lt(i, 2) { i := add(i, 1) } {
                    if eq(i, 7) { leave }
                }
                value := load_nonzero(offset)
                if value { leave }
            }
        }
    }
}
