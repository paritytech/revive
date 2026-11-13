object "MloadDynamicOffsetMsize" {
    code {
        datacopy(0, dataoffset("MloadDynamicOffsetMsize_deployed"), datasize("MloadDynamicOffsetMsize_deployed"))
        return(0, datasize("MloadDynamicOffsetMsize_deployed"))
    }
    object "MloadDynamicOffsetMsize_deployed" {
        code {
            let x := mload(calldataload(0))
            mstore(0, msize())
            return(0, 0x20)
        }
    }
}
