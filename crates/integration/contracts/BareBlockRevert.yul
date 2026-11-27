object "BareBlockRevert" {
    code {
        datacopy(0, dataoffset("BareBlockRevert_deployed"), datasize("BareBlockRevert_deployed"))
        return(0, datasize("BareBlockRevert_deployed"))
    }
    object "BareBlockRevert_deployed" {
        code {
            for { } 1 { } {
                mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
                let value := 1
                mstore(4, 0x11)
                { revert(0, 0x24) }
                sstore(0, value)
            }
        }
    }
}
