object "BareBlockInLoop" {
    code {
        datacopy(0, dataoffset("BareBlockInLoop_deployed"), datasize("BareBlockInLoop_deployed"))
        return(0, datasize("BareBlockInLoop_deployed"))
    }
    object "BareBlockInLoop_deployed" {
        code {
            let value := 0
            for { } 1 { } {
                { value := 3 }
                break
            }
            mstore(0, value)
            return(0, 32)
        }
    }
}
