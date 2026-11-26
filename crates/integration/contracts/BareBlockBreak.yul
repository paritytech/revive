object "BareBlockBreak" {
    code {
        datacopy(0, dataoffset("BareBlockBreak_deployed"), datasize("BareBlockBreak_deployed"))
        return(0, datasize("BareBlockBreak_deployed"))
    }
    object "BareBlockBreak_deployed" {
        code {
            for { } 1 { } {
                mstore(0, 7)
                { break }
                mstore(0, 9)
            }
            return(0, 32)
        }
    }
}
