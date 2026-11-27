object "BareBlockContinue" {
    code {
        datacopy(0, dataoffset("BareBlockContinue_deployed"), datasize("BareBlockContinue_deployed"))
        return(0, datasize("BareBlockContinue_deployed"))
    }
    object "BareBlockContinue_deployed" {
        code {
            for { let i := 1 } i { i := 0 } {
                mstore(0, 7)
                { continue }
                mstore(0, 9)
            }
            return(0, 32)
        }
    }
}
