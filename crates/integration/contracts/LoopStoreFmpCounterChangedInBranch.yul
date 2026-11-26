object "LoopStoreFmpCounterChangedInBranch" {
    code {
        datacopy(0x80, dataoffset("LoopStoreFmpCounterChangedInBranch_deployed"), datasize("LoopStoreFmpCounterChangedInBranch_deployed"))
        return(0x80, datasize("LoopStoreFmpCounterChangedInBranch_deployed"))
    }
    object "LoopStoreFmpCounterChangedInBranch_deployed" {
        code {
            for { let offset := 0x80 } lt(offset, calldataload(0x20)) { offset := add(offset, 0x20) } {
                if eq(offset, 0x80) {
                    offset := sub(offset, 0x60)
                    continue
                }
                mstore(offset, calldataload(0))
                break
            }
            if iszero(gt(mload(0x40), 0xffffffff)) { revert(0, 0) }
        }
    }
}
